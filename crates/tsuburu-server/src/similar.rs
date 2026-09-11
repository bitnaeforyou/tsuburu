//! "Scenes like this one", over the imported embeddings.
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

    /// Passages nearest to a vector that came from outside the index.
    pub fn near_vector(
        &self,
        query: &[f32],
        limit: usize,
        duplicate: &dyn Fn(i32, u16) -> Option<Vec<u8>>,
    ) -> Vec<Match> {
        let wanted = (limit * 8).clamp(limit, 400);
        let neighbours = self.index.nearest(query, wanted, &|_| false);
        let mut out = Vec::new();
        let mut seen_work = std::collections::HashSet::new();
        let mut seen_passage = std::collections::HashSet::new();
        for neighbour in neighbours {
            let Some((work, page)) = self.locator.locate(neighbour.chunk) else { continue };
            if !seen_work.insert(work) {
                continue;
            }
            if let Some(key) = duplicate(work, page)
                && !seen_passage.insert(key)
            {
                continue;
            }
            out.push(Match { gallery_id: work, page, score: neighbour.score });
            if out.len() >= limit {
                break;
            }
        }
        out
    }

    /// The stored vector for a passage, so a pack can be checked against it.
    pub fn stored_vector(&self, gallery: i32, page: u16) -> Option<Vec<f32>> {
        let chunk = self.locator.chunk_for(gallery, page)?;
        self.index.vector(chunk).ok()
    }

    pub fn dims(&self) -> usize {
        self.index.dims()
    }
}

/// Passages this machine embedded itself, scored against a query.
///
/// The imported index is a fixed file: it holds its corpus and nothing read
/// since. These are kept beside it and scanned linearly, which costs
/// nothing at the sizes reading produces.
pub fn near_local(
    store: &tsuburu_dialogue::DialogueStore,
    query: &[f32],
    limit: usize,
    exclude: Option<i32>,
) -> Vec<Match> {
    let mut found: Vec<Match> = Vec::new();
    let scan = store.each_vector(|gallery, page, vector| {
        if Some(gallery) == exclude || vector.len() != query.len() {
            return;
        }
        let score = query.iter().zip(vector).map(|(a, b)| a * b).sum();
        found.push(Match { gallery_id: gallery, page, score });
    });
    if let Err(err) = scan {
        tracing::debug!(%err, "could not scan the passages read here");
        return Vec::new();
    }
    found.sort_unstable_by(|a, b| b.score.total_cmp(&a.score));
    found.dedup_by_key(|m| m.gallery_id);
    found.truncate(limit);
    found
}

/// Two ranked lists as one, best per work.
pub fn merge(mut left: Vec<Match>, right: Vec<Match>, limit: usize) -> Vec<Match> {
    left.extend(right);
    left.sort_unstable_by(|a, b| b.score.total_cmp(&a.score));
    let mut seen = std::collections::HashSet::new();
    left.retain(|m| seen.insert(m.gallery_id));
    left.truncate(limit);
    left
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
