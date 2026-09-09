//! "Scenes like this one", over artifact's embeddings.
//!
//! No model is loaded. The query is a vector already in the index — the one
//! stored for the passage the user is looking at — so the whole thing is a
//! nearest-neighbour scan.
//!
//! Both the index (2.6 GB, memory mapped) and the chunk-to-gallery table are
//! built on first use, because most sessions never ask for this.

use std::path::Path;
use std::sync::{Arc, OnceLock};
use tsuburu_dialogue::artifact::ChunkLocator;
use tsuburu_embed::EmbeddingIndex;

pub struct Similarity {
    index: EmbeddingIndex,
    locator: ChunkLocator,
}

impl Similarity {
    fn open(dir: &Path) -> Result<Self, String> {
        let index = EmbeddingIndex::open(dir.join("faiss-sq8.index")).map_err(|e| e.to_string())?;
        let locator = ChunkLocator::build(dir).map_err(|e| e.to_string())?;
        if index.len() != locator.len() {
            return Err(format!(
                "the index has {} vectors but the text has {} chunks; they are not a pair",
                index.len(),
                locator.len()
            ));
        }
        Ok(Self { index, locator })
    }

    /// Galleries whose passages sit closest to `(gallery, page)`.
    ///
    /// One gallery contributes once, at its best passage, so a long work
    /// cannot fill the answer with its own chapters. `duplicate` folds
    /// re-uploads together: hitomi carries the same work under several ids,
    /// and every copy is equally near.
    pub fn near(
        &self,
        gallery: i32,
        page: u16,
        limit: usize,
        duplicate: &dyn Fn(i32, u16) -> Option<Vec<u8>>,
    ) -> Result<Vec<Match>, String> {
        let chunk = self
            .locator
            .chunk_for(gallery, page)
            .ok_or_else(|| format!("gallery {gallery} is not in the embedding index"))?;

        // Ask for more than needed: neighbours cluster inside one work.
        let wanted = (limit * 8).clamp(limit, 400);
        let neighbours = self
            .index
            .nearest_to_chunk(chunk, wanted, &|c| {
                self.locator.locate(c).is_some_and(|(w, _)| w == gallery)
            })
            .map_err(|e| e.to_string())?;

        let mut out: Vec<Match> = Vec::new();
        let mut seen_work = std::collections::HashSet::new();
        let mut seen_passage = std::collections::HashSet::new();
        // The source itself is excluded by work, but hitomi's re-uploads of
        // it are separate galleries that match it perfectly. Seed the
        // passage with the query's own so those drop out too.
        if let Some(key) = duplicate(gallery, page) {
            seen_passage.insert(key);
        }
        for neighbour in neighbours {
            let Some((work, first_page)) = self.locator.locate(neighbour.chunk) else { continue };
            if !seen_work.insert(work) {
                continue;
            }
            if let Some(key) = duplicate(work, first_page)
                && !seen_passage.insert(key)
            {
                continue;
            }
            out.push(Match { gallery_id: work, page: first_page, score: neighbour.score });
            if out.len() >= limit {
                break;
            }
        }
        Ok(out)
    }

    pub fn vectors(&self) -> usize {
        self.index.len()
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct Match {
    pub gallery_id: i32,
    pub page: u16,
    pub score: f32,
}

/// Opens the index once, on the first request that needs it.
#[derive(Default)]
pub struct LazySimilarity {
    cell: OnceLock<Result<Arc<Similarity>, String>>,
}

impl LazySimilarity {
    pub fn get(&self, dir: &Path) -> Result<Arc<Similarity>, String> {
        self.cell.get_or_init(|| Similarity::open(dir).map(Arc::new)).clone()
    }
}
