//! Where recognised text and the work queue live.
//!
//! One redb file. Text is stored per gallery, compressed, and searched by
//! scanning, the way artifact's message search does; there is no inverted
//! index to build or rebuild. The queue is a separate table keyed so that
//! the next job is always the first key.

use flate2::Compression;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::jamo::normalize;
use crate::matcher::Query;
use crate::shard::{Shard, ShardEntry};

/// gallery id -> JobRecord JSON
const JOBS: TableDefinition<i32, &str> = TableDefinition::new("jobs");
/// (inverted priority, added_at, gallery id) -> () ; first key is the next job
const QUEUE: TableDefinition<(u8, u64, i32), ()> = TableDefinition::new("queue");
/// gallery id -> deflated JSON of Vec<PageText>
const TEXT: TableDefinition<i32, &[u8]> = TableDefinition::new("text");
const META: TableDefinition<&str, &str> = TableDefinition::new("meta");

const SCHEMA_VERSION: &str = "1";

#[derive(Debug, thiserror::Error)]
pub enum DialogueError {
    #[error("could not open the dialogue database: {0}")]
    Open(String),
    #[error("dialogue database error: {0}")]
    Database(String),
    #[error("stored record could not be read: {0}")]
    Corrupt(String),
    #[error("this dialogue index was written by a newer tsuburu (schema {found}, expected {expected})")]
    NewerSchema { found: String, expected: String },
}

impl From<serde_json::Error> for DialogueError {
    fn from(err: serde_json::Error) -> Self {
        DialogueError::Corrupt(err.to_string())
    }
}

impl From<std::io::Error> for DialogueError {
    fn from(err: std::io::Error) -> Self {
        DialogueError::Corrupt(err.to_string())
    }
}

/// Higher runs first. Imported history outranks a hunt, which outranks the
/// background sweep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Background = 0,
    Hunt = 1,
    Imported = 2,
}

impl Priority {
    /// Queue keys sort ascending, so store the inverse.
    fn key(self) -> u8 {
        u8::MAX - self as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pending,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRecord {
    pub status: Status,
    pub priority: Priority,
    pub added_at: u64,
    #[serde(default)]
    pub finished_at: Option<u64>,
    #[serde(default)]
    pub pages: u32,
    #[serde(default)]
    pub lines: u32,
    #[serde(default)]
    pub error: Option<String>,
    /// `None` for text this machine recognised, otherwise where it came from.
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageText {
    pub page: u16,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Counts {
    pub pending: usize,
    pub done: usize,
    pub failed: usize,
}

/// What a job ended with; written with its queue removal in one commit.
struct Outcome<'a> {
    status: Status,
    text: Option<&'a [u8]>,
    pages: u32,
    lines: u32,
    error: Option<&'a str>,
    source: Option<&'a str>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ImportSummary {
    pub galleries: usize,
    pub added: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub gallery_id: i32,
    pub page: u16,
    pub score: f32,
    pub exact: bool,
    /// The matching line with a little context either side.
    pub snippet: Vec<String>,
}

pub struct DialogueStore {
    db: Database,
    path: PathBuf,
}

impl DialogueStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DialogueError> {
        let path = path.as_ref().to_path_buf();
        let db = Database::create(&path).map_err(|e| DialogueError::Open(e.to_string()))?;
        let store = Self { db, path };
        store.init_schema()?;
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn init_schema(&self) -> Result<(), DialogueError> {
        let tx = self.db.begin_write().map_err(db_err)?;
        {
            tx.open_table(JOBS).map_err(db_err)?;
            tx.open_table(QUEUE).map_err(db_err)?;
            tx.open_table(TEXT).map_err(db_err)?;
            let mut meta = tx.open_table(META).map_err(db_err)?;
            let existing = meta.get("schema").map_err(db_err)?.map(|v| v.value().to_string());
            match existing {
                None => {
                    meta.insert("schema", SCHEMA_VERSION).map_err(db_err)?;
                }
                Some(found) if found == SCHEMA_VERSION => {}
                Some(found) => {
                    return Err(DialogueError::NewerSchema { found, expected: SCHEMA_VERSION.into() });
                }
            }
        }
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    // --- settings ---

    pub fn setting(&self, key: &str) -> Result<Option<String>, DialogueError> {
        let tx = self.db.begin_read().map_err(db_err)?;
        let meta = tx.open_table(META).map_err(db_err)?;
        Ok(meta.get(key).map_err(db_err)?.map(|v| v.value().to_string()))
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), DialogueError> {
        let tx = self.db.begin_write().map_err(db_err)?;
        {
            let mut meta = tx.open_table(META).map_err(db_err)?;
            meta.insert(key, value).map_err(db_err)?;
        }
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    // --- queue ---

    /// Adds galleries to the queue. Finished ones are skipped; a pending one
    /// is promoted if the new priority is higher, never demoted.
    pub fn enqueue(&self, ids: &[i32], priority: Priority) -> Result<usize, DialogueError> {
        let now = now_millis();
        let tx = self.db.begin_write().map_err(db_err)?;
        let mut added = 0usize;
        {
            let mut jobs = tx.open_table(JOBS).map_err(db_err)?;
            let mut queue = tx.open_table(QUEUE).map_err(db_err)?;
            for &id in ids {
                let existing = jobs.get(id).map_err(db_err)?.map(|v| v.value().to_string());
                let existing: Option<JobRecord> =
                    existing.map(|s| serde_json::from_str(&s)).transpose()?;
                match existing {
                    Some(job) if job.status == Status::Done => continue,
                    Some(job) if job.status == Status::Pending && job.priority >= priority => continue,
                    Some(job) => {
                        // Promote (or retry a failure) by replacing the queue key.
                        queue.remove((job.priority.key(), job.added_at, id)).map_err(db_err)?;
                    }
                    None => {}
                }
                let record = JobRecord {
                    status: Status::Pending,
                    priority,
                    added_at: now,
                    finished_at: None,
                    pages: 0,
                    lines: 0,
                    error: None,
                    source: None,
                };
                jobs.insert(id, serde_json::to_string(&record)?.as_str()).map_err(db_err)?;
                queue.insert((priority.key(), now, id), ()).map_err(db_err)?;
                added += 1;
            }
        }
        tx.commit().map_err(db_err)?;
        Ok(added)
    }

    /// The next gallery to work on, without removing it. Marking it done or
    /// failed removes it from the queue.
    pub fn next_pending(&self) -> Result<Option<i32>, DialogueError> {
        let tx = self.db.begin_read().map_err(db_err)?;
        let queue = tx.open_table(QUEUE).map_err(db_err)?;
        let first = queue.first().map_err(db_err)?;
        Ok(first.map(|(key, _)| key.value().2))
    }

    pub fn job(&self, id: i32) -> Result<Option<JobRecord>, DialogueError> {
        let tx = self.db.begin_read().map_err(db_err)?;
        let jobs = tx.open_table(JOBS).map_err(db_err)?;
        match jobs.get(id).map_err(db_err)? {
            Some(v) => Ok(Some(serde_json::from_str(v.value())?)),
            None => Ok(None),
        }
    }

    /// Stores the recognised text and marks the job done in one commit.
    pub fn complete(&self, id: i32, pages: &[PageText]) -> Result<(), DialogueError> {
        let encoded = compress(&serde_json::to_vec(pages)?)?;
        let lines: usize = pages.iter().map(|p| p.lines.len()).sum();
        self.finish(
            id,
            Outcome { status: Status::Done, text: Some(&encoded), pages: pages.len() as u32, lines: lines as u32, error: None, source: None },
        )
    }

    pub fn fail(&self, id: i32, error: &str) -> Result<(), DialogueError> {
        self.finish(
            id,
            Outcome { status: Status::Failed, text: None, pages: 0, lines: 0, error: Some(error), source: None },
        )
    }

    fn finish(&self, id: i32, outcome: Outcome<'_>) -> Result<(), DialogueError> {
        let Outcome { status, text, pages, lines, error, source } = outcome;
        let tx = self.db.begin_write().map_err(db_err)?;
        {
            let mut jobs = tx.open_table(JOBS).map_err(db_err)?;
            let mut queue = tx.open_table(QUEUE).map_err(db_err)?;
            let mut texts = tx.open_table(TEXT).map_err(db_err)?;

            let previous: Option<JobRecord> = jobs
                .get(id)
                .map_err(db_err)?
                .map(|v| serde_json::from_str(v.value()))
                .transpose()?;
            let (priority, added_at) = previous
                .as_ref()
                .map(|j| (j.priority, j.added_at))
                .unwrap_or((Priority::Background, now_millis()));
            queue.remove((priority.key(), added_at, id)).map_err(db_err)?;

            let record = JobRecord {
                status,
                priority,
                added_at,
                finished_at: Some(now_millis()),
                pages,
                lines,
                error: error.map(str::to_string),
                source: source.map(str::to_string),
            };
            jobs.insert(id, serde_json::to_string(&record)?.as_str()).map_err(db_err)?;
            if let Some(bytes) = text {
                texts.insert(id, bytes).map_err(db_err)?;
            }
        }
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    pub fn counts(&self) -> Result<Counts, DialogueError> {
        let tx = self.db.begin_read().map_err(db_err)?;
        let jobs = tx.open_table(JOBS).map_err(db_err)?;
        let mut counts = Counts::default();
        for row in jobs.iter().map_err(db_err)? {
            let (_, value) = row.map_err(db_err)?;
            let job: JobRecord = serde_json::from_str(value.value())?;
            match job.status {
                Status::Pending => counts.pending += 1,
                Status::Done => counts.done += 1,
                Status::Failed => counts.failed += 1,
            }
        }
        Ok(counts)
    }

    /// Ids whose text is stored. Lets the caller measure coverage against
    /// an ordering it knows about, such as popularity.
    pub fn done_ids(&self) -> Result<std::collections::HashSet<i32>, DialogueError> {
        let tx = self.db.begin_read().map_err(db_err)?;
        let texts = tx.open_table(TEXT).map_err(db_err)?;
        let mut out = std::collections::HashSet::new();
        for row in texts.iter().map_err(db_err)? {
            let (key, _) = row.map_err(db_err)?;
            out.insert(key.value());
        }
        Ok(out)
    }

    pub fn text(&self, id: i32) -> Result<Option<Vec<PageText>>, DialogueError> {
        let tx = self.db.begin_read().map_err(db_err)?;
        let texts = tx.open_table(TEXT).map_err(db_err)?;
        match texts.get(id).map_err(db_err)? {
            Some(v) => Ok(Some(serde_json::from_slice(&decompress(v.value())?)?)),
            None => Ok(None),
        }
    }

    // --- exchange ---

    /// Packs every stored gallery into shards of `range_size` ids.
    ///
    /// `background_only` leaves out galleries that were indexed because the
    /// user asked for them (imported history, hunts): those reveal what the
    /// user read, and a shared file must not.
    pub fn export_shards(
        &self,
        range_size: i32,
        background_only: bool,
    ) -> Result<Vec<Shard>, DialogueError> {
        let range_size = range_size.max(1);
        let tx = self.db.begin_read().map_err(db_err)?;
        let texts = tx.open_table(TEXT).map_err(db_err)?;
        let jobs = tx.open_table(JOBS).map_err(db_err)?;

        let mut shards: Vec<Shard> = Vec::new();
        for row in texts.iter().map_err(db_err)? {
            let (key, value) = row.map_err(db_err)?;
            let id = key.value();
            if background_only {
                let job: Option<JobRecord> = jobs
                    .get(id)
                    .map_err(db_err)?
                    .map(|v| serde_json::from_str(v.value()))
                    .transpose()?;
                let personal = job.is_some_and(|j| j.priority != Priority::Background);
                if personal {
                    continue;
                }
            }
            let pages: Vec<PageText> = serde_json::from_slice(&decompress(value.value())?)?;
            let first_id = (id / range_size) * range_size;
            let shard = match shards.last_mut() {
                Some(last) if last.first_id == first_id => last,
                _ => {
                    shards.push(Shard {
                        first_id,
                        last_id: first_id + range_size - 1,
                        entries: Vec::new(),
                    });
                    shards.last_mut().expect("just pushed")
                }
            };
            shard.entries.push(ShardEntry { gallery_id: id, pages });
        }
        Ok(shards)
    }

    /// Merges a shard. Galleries already indexed here are left alone.
    pub fn import_shard(&self, shard: &Shard) -> Result<ImportSummary, DialogueError> {
        let existing = self.done_ids()?;
        let mut summary = ImportSummary { galleries: shard.entries.len(), added: 0, skipped: 0 };
        for entry in &shard.entries {
            if existing.contains(&entry.gallery_id) || entry.pages.is_empty() {
                summary.skipped += 1;
                continue;
            }
            let encoded = compress(&serde_json::to_vec(&entry.pages)?)?;
            let lines: usize = entry.pages.iter().map(|p| p.lines.len()).sum();
            self.finish(
                entry.gallery_id,
                Outcome {
                    status: Status::Done,
                    text: Some(&encoded),
                    pages: entry.pages.len() as u32,
                    lines: lines as u32,
                    error: None,
                    source: Some("import"),
                },
            )?;
            summary.added += 1;
        }
        Ok(summary)
    }

    // --- search ---

    /// Scans every stored gallery. Returns the best page per gallery, best
    /// galleries first, at most `limit`.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<Hit>, DialogueError> {
        let query = Query::new(query);
        if query.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let tx = self.db.begin_read().map_err(db_err)?;
        let texts = tx.open_table(TEXT).map_err(db_err)?;

        let mut hits: Vec<Hit> = Vec::new();
        for row in texts.iter().map_err(db_err)? {
            let (key, value) = row.map_err(db_err)?;
            let gallery_id = key.value();
            let pages: Vec<PageText> = match decompress(value.value())
                .and_then(|raw| serde_json::from_slice(&raw).map_err(Into::into))
            {
                Ok(pages) => pages,
                Err(err) => {
                    tracing::warn!(gallery_id, %err, "skipping unreadable dialogue record");
                    continue;
                }
            };

            let mut best: Option<Hit> = None;
            for page in &pages {
                // Lines are joined so a phrase split across bubbles still matches.
                let joined = normalize(&page.lines.join(""));
                let Some(m) = query.score(&joined) else { continue };
                if best.as_ref().is_none_or(|b| m.score > b.score) {
                    best = Some(Hit {
                        gallery_id,
                        page: page.page,
                        score: m.score,
                        exact: m.exact,
                        snippet: snippet(&page.lines, &query),
                    });
                }
                if m.exact {
                    break;
                }
            }
            if let Some(hit) = best {
                hits.push(hit);
            }
        }

        hits.sort_by(|a, b| {
            b.exact
                .cmp(&a.exact)
                .then(b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal))
                .then(b.gallery_id.cmp(&a.gallery_id))
        });
        hits.truncate(limit);
        Ok(hits)
    }
}

/// The line that matches best plus one line either side.
fn snippet(lines: &[String], query: &Query) -> Vec<String> {
    let mut best_index = 0usize;
    let mut best_score = -1.0f32;
    for i in 0..lines.len() {
        // Score each line together with its neighbours so a phrase that
        // straddles two lines is attributed to the right spot.
        let window: String = lines[i.saturating_sub(1)..(i + 2).min(lines.len())].join("");
        if let Some(m) = query.score(&normalize(&window))
            && m.score > best_score
        {
            best_score = m.score;
            best_index = i;
        }
    }
    let start = best_index.saturating_sub(1);
    let end = (best_index + 2).min(lines.len());
    lines[start..end].to_vec()
}

fn compress(raw: &[u8]) -> Result<Vec<u8>, DialogueError> {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(raw)?;
    Ok(encoder.finish()?)
}

fn decompress(encoded: &[u8]) -> Result<Vec<u8>, DialogueError> {
    let mut out = Vec::new();
    DeflateDecoder::new(encoded).read_to_end(&mut out)?;
    Ok(out)
}

fn db_err(err: impl std::fmt::Display) -> DialogueError {
    DialogueError::Database(err.to_string())
}

fn now_millis() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (DialogueStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        (DialogueStore::open(dir.path().join("d.redb")).unwrap(), dir)
    }

    fn pages(texts: &[&[&str]]) -> Vec<PageText> {
        texts
            .iter()
            .enumerate()
            .map(|(i, lines)| PageText {
                page: i as u16,
                lines: lines.iter().map(|s| s.to_string()).collect(),
            })
            .collect()
    }

    #[test]
    fn queue_serves_higher_priority_first_then_oldest() {
        let (store, _d) = store();
        store.enqueue(&[10, 11], Priority::Background).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        store.enqueue(&[20], Priority::Hunt).unwrap();
        store.enqueue(&[30], Priority::Imported).unwrap();

        assert_eq!(store.next_pending().unwrap(), Some(30));
        store.fail(30, "x").unwrap();
        assert_eq!(store.next_pending().unwrap(), Some(20));
        store.complete(20, &pages(&[&["a"]])).unwrap();
        assert_eq!(store.next_pending().unwrap(), Some(10));
        store.complete(10, &pages(&[&["a"]])).unwrap();
        assert_eq!(store.next_pending().unwrap(), Some(11));
        store.complete(11, &pages(&[&["a"]])).unwrap();
        assert_eq!(store.next_pending().unwrap(), None);
    }

    #[test]
    fn enqueue_promotes_but_never_demotes_and_skips_done() {
        let (store, _d) = store();
        store.enqueue(&[1], Priority::Hunt).unwrap();
        assert_eq!(store.enqueue(&[1], Priority::Background).unwrap(), 0);
        assert_eq!(store.job(1).unwrap().unwrap().priority, Priority::Hunt);

        assert_eq!(store.enqueue(&[1], Priority::Imported).unwrap(), 1);
        assert_eq!(store.job(1).unwrap().unwrap().priority, Priority::Imported);

        store.complete(1, &pages(&[&["a"]])).unwrap();
        assert_eq!(store.enqueue(&[1], Priority::Imported).unwrap(), 0, "done stays done");
        assert_eq!(store.next_pending().unwrap(), None);
    }

    #[test]
    fn a_failed_job_can_be_requeued() {
        let (store, _d) = store();
        store.enqueue(&[5], Priority::Background).unwrap();
        store.fail(5, "network").unwrap();
        assert_eq!(store.job(5).unwrap().unwrap().status, Status::Failed);
        assert_eq!(store.enqueue(&[5], Priority::Background).unwrap(), 1);
        assert_eq!(store.next_pending().unwrap(), Some(5));
    }

    #[test]
    fn text_round_trips_through_compression() {
        let (store, _d) = store();
        let p = pages(&[&["일단", "구급차라도"], &["좋겠어요"]]);
        store.enqueue(&[7], Priority::Background).unwrap();
        store.complete(7, &p).unwrap();
        assert_eq!(store.text(7).unwrap().unwrap(), p);
        let job = store.job(7).unwrap().unwrap();
        assert_eq!((job.pages, job.lines), (2, 3));
    }

    #[test]
    fn search_finds_exact_and_fuzzy_hits_and_ranks_exact_first() {
        let (store, _d) = store();
        store.enqueue(&[1, 2, 3], Priority::Background).unwrap();
        store.complete(1, &pages(&[&["오늘 날씨가", "참 좋네요"]])).unwrap();
        store.complete(2, &pages(&[&["일단", "구급차라도", "부르눈 게", "좋겠어요"]])).unwrap();
        store.complete(3, &pages(&[&["뭐라고?"], &["일단 구급차라도", "부르는 게 좋겠어요."]])).unwrap();

        let hits = store.search("구급차라도 부르는 게 좋겠어요", 10).unwrap();
        assert_eq!(hits.iter().map(|h| h.gallery_id).collect::<Vec<_>>(), vec![3, 2]);
        assert!(hits[0].exact);
        assert_eq!(hits[0].page, 1);
        assert!(!hits[1].exact);
        assert!(hits[0].snippet.iter().any(|l| l.contains("구급차")));
    }

    #[test]
    fn search_respects_the_limit_and_empty_query() {
        let (store, _d) = store();
        store.enqueue(&[1, 2], Priority::Background).unwrap();
        store.complete(1, &pages(&[&["안녕하세요"]])).unwrap();
        store.complete(2, &pages(&[&["안녕하세요"]])).unwrap();
        assert_eq!(store.search("안녕하세요", 1).unwrap().len(), 1);
        assert!(store.search("", 10).unwrap().is_empty());
    }

    #[test]
    fn counts_and_done_ids() {
        let (store, _d) = store();
        store.enqueue(&[1, 2, 3], Priority::Background).unwrap();
        store.complete(1, &pages(&[&["a"]])).unwrap();
        store.fail(2, "x").unwrap();
        let c = store.counts().unwrap();
        assert_eq!((c.pending, c.done, c.failed), (1, 1, 1));
        assert_eq!(store.done_ids().unwrap(), [1].into_iter().collect());
    }

    #[test]
    fn export_groups_by_id_range_and_import_merges_without_overwriting() {
        let (a, _da) = store();
        a.enqueue(&[5, 150_007, 150_008], Priority::Background).unwrap();
        a.complete(5, &pages(&[&["첫째"]])).unwrap();
        a.complete(150_007, &pages(&[&["둘째"]])).unwrap();
        a.complete(150_008, &pages(&[&["셋째"]])).unwrap();

        let shards = a.export_shards(100_000, false).unwrap();
        assert_eq!(shards.len(), 2);
        assert_eq!((shards[0].first_id, shards[0].last_id), (0, 99_999));
        assert_eq!((shards[1].first_id, shards[1].last_id), (100_000, 199_999));
        assert_eq!(shards[1].entries.len(), 2);

        let (b, _db) = store();
        b.enqueue(&[150_007], Priority::Background).unwrap();
        b.complete(150_007, &pages(&[&["내 것"]])).unwrap();
        b.enqueue(&[150_008], Priority::Hunt).unwrap();

        let summary = b.import_shard(&shards[1]).unwrap();
        assert_eq!((summary.galleries, summary.added, summary.skipped), (2, 1, 1));
        // Local text wins; the imported one fills the gap and clears its queue entry.
        assert_eq!(b.text(150_007).unwrap().unwrap()[0].lines, vec!["내 것"]);
        assert_eq!(b.text(150_008).unwrap().unwrap()[0].lines, vec!["셋째"]);
        assert_eq!(b.next_pending().unwrap(), None);
        assert_eq!(b.job(150_008).unwrap().unwrap().source.as_deref(), Some("import"));
        assert!(b.search("셋째", 5).unwrap().iter().any(|h| h.gallery_id == 150_008));
    }

    #[test]
    fn background_only_export_keeps_personal_galleries_out() {
        let (store, _d) = store();
        store.enqueue(&[1], Priority::Background).unwrap();
        store.enqueue(&[2], Priority::Imported).unwrap();
        store.enqueue(&[3], Priority::Hunt).unwrap();
        for id in [1, 2, 3] {
            store.complete(id, &pages(&[&["x"]])).unwrap();
        }
        let all: usize = store.export_shards(1_000, false).unwrap().iter().map(|s| s.entries.len()).sum();
        let public: Vec<i32> = store
            .export_shards(1_000, true)
            .unwrap()
            .iter()
            .flat_map(|s| s.entries.iter().map(|e| e.gallery_id))
            .collect();
        assert_eq!(all, 3);
        assert_eq!(public, vec![1]);
    }

    #[test]
    fn settings_persist() {
        let (store, _d) = store();
        assert_eq!(store.setting("grinder").unwrap(), None);
        store.set_setting("grinder", "{\"enabled\":true}").unwrap();
        assert_eq!(store.setting("grinder").unwrap().as_deref(), Some("{\"enabled\":true}"));
    }

    #[test]
    fn refuses_a_newer_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("d.redb");
        {
            let store = DialogueStore::open(&path).unwrap();
            let tx = store.db.begin_write().unwrap();
            tx.open_table(META).unwrap().insert("schema", "99").unwrap();
            tx.commit().unwrap();
        }
        assert!(matches!(DialogueStore::open(&path), Err(DialogueError::NewerSchema { .. })));
    }
}
