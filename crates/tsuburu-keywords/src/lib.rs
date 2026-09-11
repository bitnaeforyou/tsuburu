//! The words a work is about, out of a published `graph.csv`.
//!
//! Someone ran TF-IDF over the dialogue they recognised and kept each work's
//! hundred strongest words. That is a much smaller thing than an embedding -
//! a few tens of megabytes against 2.6 GB - and it answers two questions the
//! embeddings otherwise answer: what is this work about, and what else reads
//! like it. A shared word carries weight from both sides, so two works score
//! against each other by the sum of the products of their scores.
//!
//! Words that nearly every work uses say nothing about any of them, so the
//! import drops anything above [`MAX_DOCUMENT_FREQUENCY`]: they cost the most
//! to store and score, and carry the least.

use redb::{
    Database, MultimapTableDefinition, ReadableTable, ReadableTableMetadata, TableDefinition,
};
use std::collections::HashMap;
use std::path::Path;

/// id -> LZ4 of the work's words and scores.
const WORKS: TableDefinition<i32, &[u8]> = TableDefinition::new("works");
/// word -> (rank key, id). The rank key is the score inverted, so redb's
/// ascending order over the value walks the strongest works first and a
/// query can stop early.
const WORDS: MultimapTableDefinition<&str, (u32, i32)> = MultimapTableDefinition::new("words");
/// A word dropped for being everywhere, and how many works held it. Kept so
/// that a search for it can say why there is nothing rather than implying
/// the word appears nowhere.
const COMMON: TableDefinition<&str, u32> = TableDefinition::new("common");
const META: TableDefinition<&str, &str> = TableDefinition::new("meta");
/// 2: words dropped as too common are remembered.
const SCHEMA_VERSION: &str = "2";
/// Page cache. Posting lists are small and revisited, so this is mostly
/// enough to keep the hot words resident.
const CACHE_BYTES: usize = 64 * 1024 * 1024;

/// A word held by more works than this says nothing about any of them.
pub const MAX_DOCUMENT_FREQUENCY: u32 = 20_000;
/// Kept per work. The file carries a hundred; the tail is noise from one page.
pub const KEEP_PER_WORK: usize = 40;
/// Scanned per word when looking for neighbours.
const POSTINGS_PER_WORD: usize = 4_000;
/// Neighbours are ranked this many deep before re-uploads are folded out.
const FOLD_HEADROOM: usize = 4;

#[derive(Debug, thiserror::Error)]
pub enum KeywordError {
    #[error("could not open the keyword database: {0}")]
    Open(String),
    #[error("keyword database error: {0}")]
    Database(String),
    #[error("stored record could not be read")]
    Corrupt,
    #[error("this keyword index was written by a newer tsuburu (schema {found})")]
    NewerSchema { found: String },
    #[error("this keyword index uses an old layout (schema {found}); import it again")]
    OlderSchema { found: String },
    #[error("could not read {0}: {1}")]
    Io(String, String),
    #[error("{0} is not a keyword graph: {1}")]
    NotGraphCsv(String, String),
}

fn db_err(err: impl std::fmt::Display) -> KeywordError {
    KeywordError::Database(err.to_string())
}

/// One word and how strongly it belongs to a work.
#[derive(Debug, Clone, PartialEq)]
pub struct Scored {
    pub word: String,
    pub score: f32,
}

/// A work near another, and the words they share.
#[derive(Debug, Clone, PartialEq)]
pub struct Neighbour {
    pub id: i32,
    pub score: f32,
    pub shared: Vec<String>,
}

pub struct KeywordStore {
    db: Database,
}

impl KeywordStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, KeywordError> {
        let db = Database::builder()
            .set_cache_size(CACHE_BYTES)
            .create(path)
            .map_err(|e| KeywordError::Open(e.to_string()))?;
        let store = Self { db };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<(), KeywordError> {
        let tx = self.db.begin_write().map_err(db_err)?;
        {
            tx.open_table(WORKS).map_err(db_err)?;
            tx.open_multimap_table(WORDS).map_err(db_err)?;
            tx.open_table(COMMON).map_err(db_err)?;
            let mut meta = tx.open_table(META).map_err(db_err)?;
            let found = meta.get("schema").map_err(db_err)?.map(|v| v.value().to_string());
            match found {
                None => {
                    meta.insert("schema", SCHEMA_VERSION).map_err(db_err)?;
                }
                Some(found) if found == SCHEMA_VERSION => {}
                Some(found) if found.as_str() < SCHEMA_VERSION => {
                    return Err(KeywordError::OlderSchema { found });
                }
                Some(found) => return Err(KeywordError::NewerSchema { found }),
            }
        }
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    pub fn len(&self) -> Result<u64, KeywordError> {
        let tx = self.db.begin_read().map_err(db_err)?;
        tx.open_table(WORKS).map_err(db_err)?.len().map_err(db_err)
    }

    pub fn is_empty(&self) -> Result<bool, KeywordError> {
        Ok(self.len()? == 0)
    }

    /// Replaces one work's words. Ids arriving twice keep the last set.
    pub fn put(&self, id: i32, words: &[Scored]) -> Result<(), KeywordError> {
        let tx = self.db.begin_write().map_err(db_err)?;
        self.put_within(&tx, id, words)?;
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    fn put_within(
        &self,
        tx: &redb::WriteTransaction,
        id: i32,
        words: &[Scored],
    ) -> Result<(), KeywordError> {
        let kept: Vec<&Scored> = words.iter().take(KEEP_PER_WORK).collect();
        let previous = {
            let mut table = tx.open_table(WORKS).map_err(db_err)?;
            let previous = match table.get(id).map_err(db_err)? {
                Some(raw) => decode(raw.value())?,
                None => Vec::new(),
            };
            table.insert(id, encode(&kept).as_slice()).map_err(db_err)?;
            previous
        };

        let mut index = tx.open_multimap_table(WORDS).map_err(db_err)?;
        // A work can arrive twice - graph.csv carries an id in two places, or
        // the file is imported again. Without this the index keeps both sets
        // and the work counts twice towards every neighbour.
        for word in previous {
            index.remove(word.word.as_str(), (rank_key(word.score), id)).map_err(db_err)?;
        }
        for word in kept {
            index.insert(word.word.as_str(), (rank_key(word.score), id)).map_err(db_err)?;
        }
        Ok(())
    }

    /// Records a word left out for being in too many works.
    pub fn note_common(&self, word: &str, works: u32) -> Result<(), KeywordError> {
        let tx = self.db.begin_write().map_err(db_err)?;
        {
            let mut table = tx.open_table(COMMON).map_err(db_err)?;
            table.insert(word, works).map_err(db_err)?;
        }
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    /// How many works held this word, if it was dropped for being everywhere.
    pub fn common(&self, word: &str) -> Result<Option<u32>, KeywordError> {
        let tx = self.db.begin_read().map_err(db_err)?;
        let table = tx.open_table(COMMON).map_err(db_err)?;
        Ok(table.get(word).map_err(db_err)?.map(|v| v.value()))
    }

    /// The words of one work, strongest first.
    pub fn of(&self, id: i32) -> Result<Option<Vec<Scored>>, KeywordError> {
        let tx = self.db.begin_read().map_err(db_err)?;
        let table = tx.open_table(WORKS).map_err(db_err)?;
        let Some(raw) = table.get(id).map_err(db_err)? else { return Ok(None) };
        Ok(Some(decode(raw.value())?))
    }

    /// Works this word belongs to most strongly.
    pub fn search(&self, word: &str, limit: usize) -> Result<Vec<(i32, f32)>, KeywordError> {
        let tx = self.db.begin_read().map_err(db_err)?;
        let index = tx.open_multimap_table(WORDS).map_err(db_err)?;
        let mut out = Vec::new();
        for entry in index.get(word).map_err(db_err)? {
            let (rank, id) = entry.map_err(db_err)?.value();
            out.push((id, from_rank_key(rank)));
            if out.len() >= limit {
                break;
            }
        }
        Ok(out)
    }

    /// Works that share the most weight with this one.
    ///
    /// Each of the work's own words is looked up and every work under it
    /// gains the product of the two scores, which is the sparse dot product
    /// of the two keyword vectors.
    pub fn near(&self, id: i32, limit: usize) -> Result<Vec<Neighbour>, KeywordError> {
        let Some(mine) = self.of(id)? else { return Ok(Vec::new()) };
        let tx = self.db.begin_read().map_err(db_err)?;
        let index = tx.open_multimap_table(WORDS).map_err(db_err)?;

        let mut totals: HashMap<i32, f32> = HashMap::new();
        let mut shared: HashMap<i32, Vec<(String, f32)>> = HashMap::new();
        for word in &mine {
            let mut seen = 0usize;
            for entry in index.get(word.word.as_str()).map_err(db_err)? {
                let (rank, other) = entry.map_err(db_err)?.value();
                seen += 1;
                if seen > POSTINGS_PER_WORD {
                    break;
                }
                if other == id {
                    continue;
                }
                let weight = word.score * from_rank_key(rank);
                *totals.entry(other).or_default() += weight;
                shared.entry(other).or_default().push((word.word.clone(), weight));
            }
        }

        let mut ranked: Vec<(i32, f32)> = totals.into_iter().collect();
        // A tie is usually the same work under two ids; keep the newer.
        ranked.sort_unstable_by(|a, b| b.1.total_cmp(&a.1).then(b.0.cmp(&a.0)));
        ranked.truncate(limit * FOLD_HEADROOM);

        // hitomi carries the same work under several ids, and re-uploads have
        // the same words in the same order. Fold them rather than filling the
        // answer with one work's copies.
        let works = tx.open_table(WORKS).map_err(db_err)?;
        let mut seen_upload = std::collections::HashSet::new();
        let mut out = Vec::with_capacity(limit);
        for (other, score) in ranked {
            if let Some(raw) = works.get(other).map_err(db_err)?
                && let Ok(fingerprint) = decode(raw.value()).map(|w| upload_key(&w))
                && !seen_upload.insert(fingerprint)
            {
                continue;
            }
            let mut words = shared.remove(&other).unwrap_or_default();
            words.sort_unstable_by(|a, b| b.1.total_cmp(&a.1));
            out.push(Neighbour {
                id: other,
                score,
                shared: words.into_iter().take(6).map(|(w, _)| w).collect(),
            });
            if out.len() >= limit {
                break;
            }
        }
        Ok(out)
    }
}

/// What a work's strongest words look like. Two uploads of one work read the
/// same, so they produce the same key.
fn upload_key(words: &[Scored]) -> String {
    words.iter().take(12).map(|w| w.word.as_str()).collect::<Vec<_>>().join("\u{1}")
}

/// Scores are positive and rarely above a few tens; a thousandth is finer
/// than the ranking needs. Inverting keeps redb's ascending values in the
/// order a reader wants.
fn rank_key(score: f32) -> u32 {
    let scaled = (score.max(0.0) * 1000.0).round().min(u32::MAX as f32 - 1.0) as u32;
    u32::MAX - scaled
}

fn from_rank_key(key: u32) -> f32 {
    (u32::MAX - key) as f32 / 1000.0
}

fn encode(words: &[&Scored]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(words.len() * 16);
    for word in words {
        let bytes = word.word.as_bytes();
        raw.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
        raw.extend_from_slice(&word.score.to_le_bytes());
        raw.extend_from_slice(bytes);
    }
    lz4_flex::compress_prepend_size(&raw)
}

fn decode(raw: &[u8]) -> Result<Vec<Scored>, KeywordError> {
    let raw = lz4_flex::decompress_size_prepended(raw).map_err(|_| KeywordError::Corrupt)?;
    let mut out = Vec::new();
    let mut at = 0usize;
    while at + 6 <= raw.len() {
        let length = u16::from_le_bytes([raw[at], raw[at + 1]]) as usize;
        let score = f32::from_le_bytes([raw[at + 2], raw[at + 3], raw[at + 4], raw[at + 5]]);
        at += 6;
        if at + length > raw.len() {
            return Err(KeywordError::Corrupt);
        }
        let word = std::str::from_utf8(&raw[at..at + length]).map_err(|_| KeywordError::Corrupt)?;
        out.push(Scored { word: word.to_string(), score });
        at += length;
    }
    Ok(out)
}

pub mod graph;

#[cfg(test)]
mod tests {
    use super::*;

    fn scored(pairs: &[(&str, f32)]) -> Vec<Scored> {
        pairs.iter().map(|(w, s)| Scored { word: (*w).into(), score: *s }).collect()
    }

    fn store() -> (tempfile::TempDir, KeywordStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = KeywordStore::open(dir.path().join("keywords.redb")).unwrap();
        (dir, store)
    }

    #[test]
    fn a_work_keeps_its_words_and_scores() {
        let (_dir, store) = store();
        store.put(10, &scored(&[("에미", 20.4), ("보스", 18.2)])).unwrap();
        let words = store.of(10).unwrap().unwrap();
        assert_eq!(words[0].word, "에미");
        assert!((words[0].score - 20.4).abs() < 0.01);
        assert_eq!(words[1].word, "보스");
        assert_eq!(store.of(11).unwrap(), None);
    }

    #[test]
    fn only_the_strongest_words_are_kept() {
        let (_dir, store) = store();
        let many: Vec<Scored> = (0..KEEP_PER_WORK + 10)
            .map(|i| Scored { word: format!("w{i}"), score: 100.0 - i as f32 })
            .collect();
        store.put(1, &many).unwrap();
        assert_eq!(store.of(1).unwrap().unwrap().len(), KEEP_PER_WORK);
        // The tail is gone from the index too, not just the record.
        assert!(store.search(&format!("w{}", KEEP_PER_WORK + 5), 10).unwrap().is_empty());
    }

    #[test]
    fn search_puts_the_work_the_word_belongs_to_most_first() {
        let (_dir, store) = store();
        store.put(1, &scored(&[("보스", 2.0)])).unwrap();
        store.put(2, &scored(&[("보스", 30.0)])).unwrap();
        store.put(3, &scored(&[("보스", 10.0)])).unwrap();
        let found = store.search("보스", 10).unwrap();
        assert_eq!(found.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [2, 3, 1]);
        assert!((found[0].1 - 30.0).abs() < 0.01);
        assert!(store.search("없는말", 10).unwrap().is_empty());
    }

    #[test]
    fn neighbours_come_from_the_weight_two_works_share() {
        let (_dir, store) = store();
        store.put(1, &scored(&[("에미", 20.0), ("보스", 10.0)])).unwrap();
        // Shares both, strongly.
        store.put(2, &scored(&[("에미", 18.0), ("보스", 12.0)])).unwrap();
        // Shares one, weakly.
        store.put(3, &scored(&[("보스", 1.0), ("교사", 30.0)])).unwrap();
        // Shares nothing.
        store.put(4, &scored(&[("교사", 30.0)])).unwrap();

        let near = store.near(1, 10).unwrap();
        assert_eq!(near.iter().map(|n| n.id).collect::<Vec<_>>(), [2, 3]);
        assert!(near[0].score > near[1].score);
        assert_eq!(near[0].shared, ["에미", "보스"]);
        assert_eq!(near[1].shared, ["보스"]);
    }

    #[test]
    fn re_uploads_of_one_work_appear_once() {
        let (_dir, store) = store();
        let words = scored(&[("에미", 20.0), ("보스", 10.0)]);
        store.put(1, &words).unwrap();
        // Two ids, the same work.
        store.put(2, &words).unwrap();
        store.put(3, &words).unwrap();
        store.put(4, &scored(&[("에미", 5.0), ("교사", 30.0)])).unwrap();

        let near = store.near(1, 10).unwrap();
        assert_eq!(near.len(), 2, "{near:?}");
        // The newest of the copies stands for them.
        assert_eq!(near[0].id, 3);
        assert_eq!(near[1].id, 4);
    }

    #[test]
    fn a_work_is_never_its_own_neighbour() {
        let (_dir, store) = store();
        store.put(1, &scored(&[("에미", 20.0)])).unwrap();
        store.put(2, &scored(&[("에미", 20.0)])).unwrap();
        assert!(store.near(1, 10).unwrap().iter().all(|n| n.id != 1));
        assert!(store.near(99, 10).unwrap().is_empty());
    }

    #[test]
    fn re_importing_a_work_replaces_its_record_and_its_postings() {
        let (_dir, store) = store();
        store.put(1, &scored(&[("에미", 20.0)])).unwrap();
        store.put(1, &scored(&[("보스", 5.0)])).unwrap();
        assert_eq!(store.of(1).unwrap().unwrap(), scored(&[("보스", 5.0)]));
        // The word it no longer holds must not still point at it.
        assert!(store.search("에미", 10).unwrap().is_empty());
        assert_eq!(store.search("보스", 10).unwrap().len(), 1);
    }

    #[test]
    fn a_work_seen_twice_counts_once_towards_a_neighbour() {
        let (_dir, store) = store();
        store.put(1, &scored(&[("에미", 10.0)])).unwrap();
        store.put(2, &scored(&[("에미", 10.0)])).unwrap();
        let once = store.near(1, 5).unwrap();
        store.put(2, &scored(&[("에미", 10.0)])).unwrap();
        let twice = store.near(1, 5).unwrap();
        assert_eq!(once, twice);
        assert_eq!(twice.len(), 1);
        assert_eq!(twice[0].shared, ["에미"]);
    }

    #[test]
    fn rank_keys_round_trip_and_order_the_right_way() {
        assert!(rank_key(30.0) < rank_key(2.0));
        assert!((from_rank_key(rank_key(20.4)) - 20.4).abs() < 0.001);
        assert_eq!(from_rank_key(rank_key(-1.0)), 0.0);
    }
}
