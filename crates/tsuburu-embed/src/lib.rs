//! Nearest-neighbour search over an imported embedding index.
//!
//! The artefact ships a FAISS `IndexScalarQuantizer` holding one 1024
//! dimension vector per chunk of recognised text, quantised to a byte per
//! dimension. Searching it by a phrase would need the 4B model that produced
//! it, which is not something to bundle. Searching it by an **existing
//! chunk** needs no model at all: take the vector already stored for a
//! passage and find the passages nearest to it.
//!
//! That answers "find scenes like this one", which is the question the
//! embeddings are actually good at.

pub mod embedder;

use memmap2::Mmap;
use std::fs::File;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum EmbedError {
    #[error("no embedding index at {0}")]
    Missing(PathBuf),
    #[error("could not read {0}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error("not a FAISS scalar-quantizer index")]
    BadMagic,
    #[error("unsupported index: {0}")]
    Unsupported(String),
    #[error("index is truncated")]
    Truncated,
    #[error("chunk {0} is out of range")]
    OutOfRange(usize),
}

/// Values FAISS writes for the layout this reader understands.
const MAGIC: &[u8; 4] = b"IxSQ";
const QT_8BIT: i32 = 0;
const METRIC_INNER_PRODUCT: i32 = 0;

pub struct EmbeddingIndex {
    map: Mmap,
    dims: usize,
    count: usize,
    codes_at: usize,
    /// Per-dimension floor and span of the quantiser.
    vmin: Vec<f32>,
    vdiff: Vec<f32>,
}

/// One neighbour: the chunk and how close it is (inner product, and the
/// vectors are normalised, so this is cosine similarity).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Neighbour {
    pub chunk: usize,
    pub score: f32,
}

impl EmbeddingIndex {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, EmbedError> {
        let path = path.as_ref().to_path_buf();
        if !path.is_file() {
            return Err(EmbedError::Missing(path));
        }
        let file = File::open(&path).map_err(|e| EmbedError::Io(path.clone(), e))?;
        // Safety: the index is a read-only artefact; a concurrent truncation
        // would be a user removing the file mid-run, which no API prevents.
        let map = unsafe { Mmap::map(&file) }.map_err(|e| EmbedError::Io(path.clone(), e))?;
        Self::parse(map)
    }

    fn parse(map: Mmap) -> Result<Self, EmbedError> {
        let mut at = 0usize;
        let bytes = &map[..];
        let take = |at: &mut usize, n: usize| -> Result<&[u8], EmbedError> {
            let end = at.checked_add(n).ok_or(EmbedError::Truncated)?;
            let slice = bytes.get(*at..end).ok_or(EmbedError::Truncated)?;
            *at = end;
            Ok(slice)
        };
        let i32_at = |at: &mut usize| -> Result<i32, EmbedError> {
            Ok(i32::from_le_bytes(take(at, 4)?.try_into().unwrap()))
        };
        let u64_at = |at: &mut usize| -> Result<u64, EmbedError> {
            Ok(u64::from_le_bytes(take(at, 8)?.try_into().unwrap()))
        };

        if take(&mut at, 4)? != MAGIC {
            return Err(EmbedError::BadMagic);
        }
        let dims = i32_at(&mut at)? as usize;
        let count = u64_at(&mut at)? as usize;
        at += 16; // two placeholders FAISS writes and ignores
        at += 1; // is_trained
        let metric = i32_at(&mut at)?;
        if metric != METRIC_INNER_PRODUCT {
            return Err(EmbedError::Unsupported(format!("metric {metric}")));
        }
        let qtype = i32_at(&mut at)?;
        if qtype != QT_8BIT {
            return Err(EmbedError::Unsupported(format!("quantiser type {qtype}")));
        }
        at += 4; // range statistic
        at += 4; // its argument
        let sq_dims = u64_at(&mut at)? as usize;
        let code_size = u64_at(&mut at)? as usize;
        if sq_dims != dims || code_size != dims {
            return Err(EmbedError::Unsupported(format!(
                "expected one byte per dimension, got {code_size} for {sq_dims} dimensions"
            )));
        }

        let trained_len = u64_at(&mut at)? as usize;
        if trained_len != dims * 2 {
            return Err(EmbedError::Unsupported(format!("{trained_len} trained values")));
        }
        let trained = take(&mut at, trained_len * 4)?;
        let floats: Vec<f32> =
            trained.as_chunks::<4>().0.iter().copied().map(f32::from_le_bytes).collect();
        let (vmin, vdiff) = floats.split_at(dims);

        let codes_len = u64_at(&mut at)? as usize;
        if codes_len != count * dims {
            return Err(EmbedError::Unsupported(format!("{codes_len} code bytes")));
        }
        let codes_at = at;
        if bytes.len() < codes_at + codes_len {
            return Err(EmbedError::Truncated);
        }

        Ok(Self { dims, count, codes_at, vmin: vmin.to_vec(), vdiff: vdiff.to_vec(), map })
    }

    pub fn dims(&self) -> usize {
        self.dims
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    fn codes(&self, chunk: usize) -> Result<&[u8], EmbedError> {
        if chunk >= self.count {
            return Err(EmbedError::OutOfRange(chunk));
        }
        let start = self.codes_at + chunk * self.dims;
        Ok(&self.map[start..start + self.dims])
    }

    /// The stored vector, dequantised.
    pub fn vector(&self, chunk: usize) -> Result<Vec<f32>, EmbedError> {
        let codes = self.codes(chunk)?;
        Ok(codes
            .iter()
            .zip(&self.vmin)
            .zip(&self.vdiff)
            .map(|((&c, &vmin), &vdiff)| vmin + (c as f32 + 0.5) * vdiff / 255.0)
            .collect())
    }

    /// Folds a query into per-dimension weights so a candidate can be scored
    /// straight from its codes.
    ///
    /// `score = Σ (vmin + (c + ½)·vdiff/255)·q = bias + Σ c·weight`
    fn weights(&self, query: &[f32]) -> (f32, Vec<f32>) {
        let mut bias = 0.0f32;
        let mut weights = Vec::with_capacity(self.dims);
        for ((&vmin, &vdiff), &q) in self.vmin.iter().zip(&self.vdiff).zip(query) {
            let step = vdiff / 255.0;
            bias += vmin * q + 0.5 * step * q;
            weights.push(step * q);
        }
        (bias, weights)
    }

    /// The `limit` chunks closest to `query`, best first.
    ///
    /// Every vector is compared: 2.5 million of them at 1024 bytes each is
    /// 2.6 GB of codes, so the work is split across cores and each thread
    /// keeps only its own best few.
    pub fn nearest(
        &self,
        query: &[f32],
        limit: usize,
        skip: &(dyn Fn(usize) -> bool + Sync),
    ) -> Vec<Neighbour> {
        if query.len() != self.dims || limit == 0 || self.count == 0 {
            return Vec::new();
        }
        let (bias, weights) = self.weights(query);
        let threads =
            std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 16);
        let per = self.count.div_ceil(threads);

        let mut all: Vec<Neighbour> = std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(threads);
            for t in 0..threads {
                let lo = t * per;
                let hi = ((t + 1) * per).min(self.count);
                if lo >= hi {
                    break;
                }
                let weights = &weights;
                handles.push(scope.spawn(move || {
                    let mut best: Vec<Neighbour> = Vec::with_capacity(limit + 1);
                    for chunk in lo..hi {
                        if skip(chunk) {
                            continue;
                        }
                        let start = self.codes_at + chunk * self.dims;
                        let codes = &self.map[start..start + self.dims];
                        let score = bias + dot(codes, weights);
                        if best.len() == limit && score <= best[best.len() - 1].score {
                            continue;
                        }
                        let at = best
                            .binary_search_by(|probe| {
                                score.partial_cmp(&probe.score).unwrap_or(std::cmp::Ordering::Equal)
                            })
                            .unwrap_or_else(|at| at);
                        best.insert(at, Neighbour { chunk, score });
                        best.truncate(limit);
                    }
                    best
                }));
            }
            let mut all = Vec::new();
            for handle in handles {
                match handle.join() {
                    Ok(part) => all.extend(part),
                    Err(_) => tracing::warn!("an embedding search thread panicked"),
                }
            }
            all
        });

        all.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        all.truncate(limit);
        all
    }

    /// Neighbours of a chunk already in the index, itself excluded.
    pub fn nearest_to_chunk(
        &self,
        chunk: usize,
        limit: usize,
        skip: &(dyn Fn(usize) -> bool + Sync),
    ) -> Result<Vec<Neighbour>, EmbedError> {
        let query = self.vector(chunk)?;
        Ok(self.nearest(&query, limit, &|c| c == chunk || skip(c)))
    }
}

/// Written so the compiler can vectorise it; this runs 2.6 billion times a query.
fn dot(codes: &[u8], weights: &[f32]) -> f32 {
    let mut acc = [0.0f32; 8];
    let (code_blocks, code_rest) = codes.as_chunks::<8>();
    let (weight_blocks, weight_rest) = weights.as_chunks::<8>();
    for (c, w) in code_blocks.iter().zip(weight_blocks) {
        for lane in 0..8 {
            acc[lane] += c[lane] as f32 * w[lane];
        }
    }
    let mut total: f32 = acc.iter().sum();
    for (c, w) in code_rest.iter().zip(weight_rest) {
        total += *c as f32 * *w;
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Builds a file in the layout FAISS writes, with `vectors` quantised.
    fn write_index(path: &Path, dims: usize, vectors: &[Vec<f32>]) {
        let mut vmin = vec![f32::MAX; dims];
        let mut vmax = vec![f32::MIN; dims];
        for v in vectors {
            for j in 0..dims {
                vmin[j] = vmin[j].min(v[j]);
                vmax[j] = vmax[j].max(v[j]);
            }
        }
        let vdiff: Vec<f32> = (0..dims).map(|j| (vmax[j] - vmin[j]).max(f32::EPSILON)).collect();

        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(dims as i32).to_le_bytes());
        out.extend_from_slice(&(vectors.len() as u64).to_le_bytes());
        out.extend_from_slice(&(1u64 << 20).to_le_bytes());
        out.extend_from_slice(&(1u64 << 20).to_le_bytes());
        out.push(1);
        out.extend_from_slice(&0i32.to_le_bytes()); // inner product
        out.extend_from_slice(&0i32.to_le_bytes()); // 8-bit
        out.extend_from_slice(&0i32.to_le_bytes()); // range statistic
        out.extend_from_slice(&0f32.to_le_bytes());
        out.extend_from_slice(&(dims as u64).to_le_bytes());
        out.extend_from_slice(&(dims as u64).to_le_bytes());
        out.extend_from_slice(&((dims * 2) as u64).to_le_bytes());
        for v in vmin.iter().chain(vdiff.iter()) {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&((vectors.len() * dims) as u64).to_le_bytes());
        for v in vectors {
            for j in 0..dims {
                let scaled = ((v[j] - vmin[j]) / vdiff[j] * 255.0).round().clamp(0.0, 255.0);
                out.push(scaled as u8);
            }
        }
        File::create(path).unwrap().write_all(&out).unwrap();
    }

    fn unit(dims: usize, seed: u64) -> Vec<f32> {
        let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let mut v: Vec<f32> = (0..dims)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state % 2000) as f32 / 1000.0 - 1.0
            })
            .collect();
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        for x in &mut v {
            *x /= norm;
        }
        v
    }

    #[test]
    fn reads_the_layout_and_reconstructs_vectors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("i.index");
        let dims = 64;
        let vectors: Vec<Vec<f32>> = (0..40).map(|i| unit(dims, i)).collect();
        write_index(&path, dims, &vectors);

        let index = EmbeddingIndex::open(&path).unwrap();
        assert_eq!((index.dims(), index.len()), (dims, 40));

        let restored = index.vector(3).unwrap();
        let error: f32 =
            restored.iter().zip(&vectors[3]).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
        assert!(error < 0.02, "quantisation error {error}");
    }

    #[test]
    fn a_chunk_is_nearest_to_its_own_neighbourhood() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("i.index");
        let dims = 64;
        let mut vectors: Vec<Vec<f32>> = (0..30).map(|i| unit(dims, i + 100)).collect();
        // Make 7 a near-copy of 3 so it has to come first.
        vectors[7] = vectors[3].iter().map(|x| x + 0.001).collect();
        write_index(&path, dims, &vectors);

        let index = EmbeddingIndex::open(&path).unwrap();
        let hits = index.nearest_to_chunk(3, 3, &|_| false).unwrap();
        assert_eq!(hits[0].chunk, 7, "the near-copy leads");
        assert!(hits[0].score > 0.9, "score {}", hits[0].score);
        assert!(hits.iter().all(|h| h.chunk != 3), "a chunk is not its own neighbour");
        assert!(hits.windows(2).all(|w| w[0].score >= w[1].score), "sorted");
    }

    #[test]
    fn skipped_chunks_are_left_out() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("i.index");
        let vectors: Vec<Vec<f32>> = (0..20).map(|i| unit(32, i)).collect();
        write_index(&path, 32, &vectors);
        let index = EmbeddingIndex::open(&path).unwrap();

        let hits = index.nearest_to_chunk(0, 5, &|c| c % 2 == 1).unwrap();
        assert!(hits.iter().all(|h| h.chunk % 2 == 0));
    }

    #[test]
    fn rejects_files_that_are_not_this_index() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nope.index");
        File::create(&path).unwrap().write_all(b"not an index at all").unwrap();
        assert!(matches!(EmbeddingIndex::open(&path), Err(EmbedError::BadMagic)));
        assert!(matches!(
            EmbeddingIndex::open(dir.path().join("absent")),
            Err(EmbedError::Missing(_))
        ));
    }
}
