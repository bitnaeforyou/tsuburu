//! Reader for artifact's `llm-search-index` artefact.
//!
//! `compact-metadata.bin` holds the recognised Korean text of the whole
//! corpus as chunks: each covers a sliding window of three pages and carries
//! the lines of those pages separated by newlines. `compact-metadata-offsets.bin`
//! is one little-endian u64 per chunk plus a final end offset.
//!
//! Record layout (little-endian): `i64 work_id, u16 page_count, u32 text_len`,
//! then `page_count` u32 page numbers (1-based), then `text_len` bytes of
//! UTF-8. Chunks of one work are stored contiguously.
//!
//! The FSCM file in the other artefact was considered and rejected as the
//! text source: its `message` field is the keyboard-keystroke transliteration
//! artifact matches on (`dkssudgktpdy` for 안녕하세요), and this build carries
//! no raw text, so nothing there could be shown to a reader.

use crate::store::PageText;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};

const HEADER_LEN: usize = 14;

#[derive(Debug, thiserror::Error)]
pub enum ArtifactError {
    #[error("not a artifact llm-search-index directory: {0} is missing")]
    Missing(PathBuf),
    #[error("could not read {0}: {1}")]
    Io(PathBuf, io::Error),
    #[error("corrupt record at chunk {0}: {1}")]
    Corrupt(usize, String),
}

/// One recognised chunk, before grouping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub work_id: i64,
    /// 1-based page numbers this chunk spans.
    pub pages: Vec<u32>,
    pub text: String,
}

pub fn parse_record(bytes: &[u8]) -> Result<Chunk, String> {
    if bytes.len() < HEADER_LEN {
        return Err("shorter than the header".into());
    }
    let work_id = i64::from_le_bytes(bytes[0..8].try_into().unwrap());
    let page_count = u16::from_le_bytes([bytes[8], bytes[9]]) as usize;
    let text_len = u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize;
    let pages_end = HEADER_LEN + page_count * 4;
    if bytes.len() != pages_end + text_len {
        return Err(format!(
            "length {} does not match header ({})",
            bytes.len(),
            pages_end + text_len
        ));
    }
    let pages = bytes[HEADER_LEN..pages_end]
        .as_chunks::<4>()
        .0
        .iter()
        .copied()
        .map(u32::from_le_bytes)
        .collect();
    let text = String::from_utf8_lossy(&bytes[pages_end..]).into_owned();
    Ok(Chunk { work_id, pages, text })
}

/// Streams chunks in file order.
pub struct ChunkReader {
    records: BufReader<File>,
    offsets: Vec<u64>,
    index: usize,
    buffer: Vec<u8>,
}

impl ChunkReader {
    pub fn open(dir: &Path) -> Result<Self, ArtifactError> {
        let offsets_path = dir.join("compact-metadata-offsets.bin");
        let records_path = dir.join("compact-metadata.bin");
        for path in [&offsets_path, &records_path] {
            if !path.is_file() {
                return Err(ArtifactError::Missing(path.clone()));
            }
        }
        let raw =
            std::fs::read(&offsets_path).map_err(|e| ArtifactError::Io(offsets_path.clone(), e))?;
        let offsets: Vec<u64> =
            raw.as_chunks::<8>().0.iter().copied().map(u64::from_le_bytes).collect();
        let file =
            File::open(&records_path).map_err(|e| ArtifactError::Io(records_path.clone(), e))?;
        Ok(Self {
            records: BufReader::with_capacity(4 << 20, file),
            offsets,
            index: 0,
            buffer: Vec::new(),
        })
    }

    /// Number of chunks in the file.
    pub fn len(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Iterator for ChunkReader {
    type Item = Result<Chunk, ArtifactError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index + 1 >= self.offsets.len() {
            return None;
        }
        let start = self.offsets[self.index];
        let end = self.offsets[self.index + 1];
        let i = self.index;
        self.index += 1;
        if end < start {
            return Some(Err(ArtifactError::Corrupt(i, "offsets run backwards".into())));
        }
        self.buffer.resize((end - start) as usize, 0);
        if let Err(e) = self.records.read_exact(&mut self.buffer) {
            return Some(Err(ArtifactError::Io(PathBuf::from("compact-metadata.bin"), e)));
        }
        Some(parse_record(&self.buffer).map_err(|e| ArtifactError::Corrupt(i, e)))
    }
}

/// Groups consecutive chunks of the same work into pages for the store.
///
/// A chunk spans three pages and adjacent chunks overlap by one, so the
/// same line can appear twice; the first page of the window is used as
/// the page number (0-based), which is where a reader should start.
pub struct WorkGrouper<I: Iterator<Item = Result<Chunk, ArtifactError>>> {
    chunks: std::iter::Peekable<I>,
}

impl<I: Iterator<Item = Result<Chunk, ArtifactError>>> WorkGrouper<I> {
    pub fn new(chunks: I) -> Self {
        Self { chunks: chunks.peekable() }
    }
}

impl<I: Iterator<Item = Result<Chunk, ArtifactError>>> Iterator for WorkGrouper<I> {
    type Item = Result<(i32, Vec<PageText>), ArtifactError>;

    fn next(&mut self) -> Option<Self::Item> {
        let first = match self.chunks.next()? {
            Ok(chunk) => chunk,
            Err(e) => return Some(Err(e)),
        };
        let work_id = first.work_id;
        let mut pages = vec![to_page(&first)];
        while let Some(Ok(next)) = self.chunks.peek() {
            if next.work_id != work_id {
                break;
            }
            let chunk = self.chunks.next().expect("peeked").expect("peeked ok");
            pages.push(to_page(&chunk));
        }
        let id = match i32::try_from(work_id) {
            Ok(id) => id,
            Err(_) => {
                return Some(Err(ArtifactError::Corrupt(
                    0,
                    format!("work id {work_id} out of range"),
                )));
            }
        };
        Some(Ok((id, pages)))
    }
}

fn to_page(chunk: &Chunk) -> PageText {
    let page =
        chunk.pages.first().copied().unwrap_or(1).saturating_sub(1).min(u16::MAX as u32) as u16;
    PageText {
        page,
        lines: chunk.text.split('\n').filter(|l| !l.is_empty()).map(str::to_string).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(work: i64, pages: &[u32], text: &str) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&work.to_le_bytes());
        out.extend_from_slice(&(pages.len() as u16).to_le_bytes());
        out.extend_from_slice(&(text.len() as u32).to_le_bytes());
        for p in pages {
            out.extend_from_slice(&p.to_le_bytes());
        }
        out.extend_from_slice(text.as_bytes());
        out
    }

    fn write_artifact(dir: &Path, records: &[Vec<u8>]) {
        let mut data = Vec::new();
        let mut offsets = vec![0u64];
        for r in records {
            data.extend_from_slice(r);
            offsets.push(data.len() as u64);
        }
        std::fs::write(dir.join("compact-metadata.bin"), data).unwrap();
        let raw: Vec<u8> = offsets.iter().flat_map(|o| o.to_le_bytes()).collect();
        std::fs::write(dir.join("compact-metadata-offsets.bin"), raw).unwrap();
    }

    #[test]
    fn parses_a_record() {
        let bytes = record(4031231, &[1, 2, 3], "싫어\nBorusiti");
        let chunk = parse_record(&bytes).unwrap();
        assert_eq!(chunk.work_id, 4031231);
        assert_eq!(chunk.pages, vec![1, 2, 3]);
        assert_eq!(chunk.text, "싫어\nBorusiti");
        assert!(parse_record(&bytes[..bytes.len() - 1]).is_err());
    }

    #[test]
    fn streams_and_groups_by_work() {
        let dir = tempfile::tempdir().unwrap();
        write_artifact(
            dir.path(),
            &[
                record(10, &[1, 2, 3], "첫 줄\n둘째 줄"),
                record(10, &[3, 4, 5], "셋째 줄"),
                record(20, &[1, 2, 3], "다른 작품"),
            ],
        );
        let reader = ChunkReader::open(dir.path()).unwrap();
        assert_eq!(reader.len(), 3);
        let works: Vec<_> = WorkGrouper::new(reader).map(Result::unwrap).collect();
        assert_eq!(works.len(), 2);
        assert_eq!(works[0].0, 10);
        assert_eq!(works[0].1.len(), 2);
        assert_eq!(works[0].1[0].page, 0);
        assert_eq!(works[0].1[1].page, 2);
        assert_eq!(works[0].1[0].lines, vec!["첫 줄", "둘째 줄"]);
        assert_eq!(works[1].0, 20);
    }

    #[test]
    fn missing_files_are_reported_clearly() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(ChunkReader::open(dir.path()), Err(ArtifactError::Missing(_))));
    }
}
