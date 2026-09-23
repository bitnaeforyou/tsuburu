//! Where recognised text and the work queue live.
//!
//! One redb file. Text is stored per gallery in a compact binary form
//! behind LZ4, and searched by scanning, the way the message search that inspired it
//! does; there is no inverted index to build or rebuild. The queue is a
//! separate table keyed so that the next job is always the first key.
//!
//! The scan is what has to stay fast. With a Korean corpus imported
//! (108k works, 1.5 GB of text) a query touches everything, so pages are
//! decoded without JSON, decompressed with LZ4 rather than deflate, scored
//! without allocating per candidate, and split across threads.

use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::matcher::{Match, Query};
use crate::shard::{Shard, ShardEntry};
use tsuburu_text::codes_into;

/// gallery id -> JobRecord JSON
const JOBS: TableDefinition<i32, &str> = TableDefinition::new("jobs");
/// (inverted priority, added_at, gallery id) -> () ; first key is the next job
const QUEUE: TableDefinition<(u8, u64, i32), ()> = TableDefinition::new("queue");
/// gallery id -> LZ4 of the binary page encoding (see `encode_pages`)
const TEXT: TableDefinition<i32, &[u8]> = TableDefinition::new("text");
/// gallery id -> LZ4 of the match codes alone (see `encode_codes`). A scan
/// reads only this table; text is fetched for the few pages that hit.
const CODES: TableDefinition<i32, &[u8]> = TableDefinition::new("codes");
/// (gallery, page) -> the passage's embedding, for pages this machine read
/// itself. an imported index covers its own corpus and nothing after it; these
/// are scanned beside it so a work you read is findable by meaning too.
const VECTORS: TableDefinition<(i32, u16), &[u8]> = TableDefinition::new("vectors");
const META: TableDefinition<&str, &str> = TableDefinition::new("meta");

/// 2: LZ4 binary pages. 3: match codes beside the text. 4: codes in their
/// own table. 5: codes cover scripts other than Hangul.
const SCHEMA_VERSION: &str = "5";
/// Page cache.
///
/// A search reads the whole codes table, so this is sized to hold it: with
/// less, every query pays for the same pages again. redb otherwise sizes the
/// cache from the machine's memory, which left a gigabyte and a half
/// resident on a large one.
const CACHE_BYTES: usize = 256 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum DialogueError {
    #[error("could not open the dialogue database: {0}")]
    Open(String),
    #[error("dialogue database error: {0}")]
    Database(String),
    #[error("stored record could not be read: {0}")]
    Corrupt(String),
    #[error(
        "this dialogue index was written by a newer tsuburu (schema {found}, expected {expected})"
    )]
    NewerSchema { found: String, expected: String },
    #[error("this dialogue index uses an old layout (schema {found}); delete it and index again")]
    OlderSchema { found: String },
}

impl From<serde_json::Error> for DialogueError {
    fn from(err: serde_json::Error) -> Self {
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
    /// True when the text was read off pages the user opened, rather than
    /// swept in the background. Old records default to false, which is what
    /// they were.
    #[serde(default)]
    pub from_reading: bool,
}

/// One gallery's stored text, as the cache screen sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stored {
    pub id: i32,
    pub pages: u32,
    pub lines: u32,
    /// Compressed text and match codes together.
    pub bytes: u64,
    pub finished_at: Option<u64>,
    pub from_reading: bool,
    pub imported: bool,
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

/// A gallery's pages, compressed twice over: text for display and codes
/// for scanning, stored in separate tables.
pub struct Encoded {
    pub text: Vec<u8>,
    pub codes: Vec<u8>,
}

impl Encoded {
    pub fn from_pages(pages: &[PageText]) -> Self {
        Self { text: compress(&encode_pages(pages)), codes: compress(&encode_codes(pages)) }
    }
}

/// What a job ended with; written with its queue removal in one commit.
struct Outcome<'a> {
    status: Status,
    text: Option<&'a Encoded>,
    pages: u32,
    lines: u32,
    error: Option<&'a str>,
    source: Option<&'a str>,
    from_reading: bool,
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
    /// Other galleries with the same passage on the same page.
    ///
    /// The same work is uploaded to hitomi more than once, and every copy
    /// matches identically. Showing them all buries the distinct results.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also: Vec<i32>,
}

/// How many answers to keep. Asking the same thing twice is common - a reader
/// tries a phrase, opens a work, comes back to the same phrase - and each
/// answer is a couple of dozen short rows.
const REMEMBERED: usize = 64;

/// How much of an answer is held for paging through.
///
/// Measured at 122 bytes a hit, most of it the snippet, so five hundred of
/// them is 60 KB and sixty-four such answers is under four megabytes - next
/// to nothing beside the store they came out of. The limit is not really
/// memory: a common phrase matches tens of thousands of works (`괜찮아` is
/// forty-six thousand), and nobody reads to the end of that. Twenty pages is
/// further than anyone goes before narrowing what they asked, and past it the
/// count still says how many there were.
pub const KEPT: usize = 500;

/// One remembered answer: the generation it was taken at, what was asked, how
/// many matched in all, and the first `KEPT` of them.
type Answer = (u64, String, usize, Vec<Hit>);

pub struct DialogueStore {
    db: Database,
    path: PathBuf,
    /// Bumped whenever what is stored changes, so anything held from before
    /// can tell it is stale rather than having to be found and cleared.
    generation: AtomicU64,
    /// The gallery ids a search partitions its threads over.
    ///
    /// Collecting them is a walk of the whole table, and on a full corpus
    /// that cost more than the search it was preparing for: over a second,
    /// against four hundred milliseconds of actual matching.
    keys: RwLock<Option<(u64, Arc<Vec<i32>>)>>,
    /// The last few answers, newest first.
    answers: Mutex<Vec<Answer>>,
}

impl DialogueStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DialogueError> {
        let path = path.as_ref().to_path_buf();
        // redb sizes its page cache from the machine's memory, which for a
        // corpus this large means a search leaves a gigabyte resident. The
        // scan reads the codes table straight through, so a cache big enough
        // to hold a working set is all it can use.
        let db = Database::builder()
            .set_cache_size(CACHE_BYTES)
            .create(&path)
            .map_err(|e| DialogueError::Open(e.to_string()))?;
        let store = Self {
            db,
            path,
            generation: AtomicU64::new(0),
            keys: RwLock::new(None),
            answers: Mutex::new(Vec::new()),
        };
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
            tx.open_table(CODES).map_err(db_err)?;
            tx.open_table(VECTORS).map_err(db_err)?;
            let mut meta = tx.open_table(META).map_err(db_err)?;
            let existing = meta.get("schema").map_err(db_err)?.map(|v| v.value().to_string());
            match existing {
                None => {
                    meta.insert("schema", SCHEMA_VERSION).map_err(db_err)?;
                }
                Some(found) if found == SCHEMA_VERSION => {}
                Some(found) if found.as_str() < SCHEMA_VERSION => {
                    return Err(DialogueError::OlderSchema { found });
                }
                Some(found) => {
                    return Err(DialogueError::NewerSchema {
                        found,
                        expected: SCHEMA_VERSION.into(),
                    });
                }
            }
        }
        tx.commit().map_err(db_err)?;
        self.changed();
        Ok(())
    }

    /// Says that what is stored has changed.
    ///
    /// Nothing is cleared here: what was held carries the generation it was
    /// taken at, and finds out for itself.
    fn changed(&self) {
        self.generation.fetch_add(1, Ordering::Release);
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// The ids to partition a search over, read once per change.
    fn search_keys(&self) -> Result<Arc<Vec<i32>>, DialogueError> {
        let now = self.generation();
        if let Ok(held) = self.keys.read()
            && let Some((at, keys)) = held.as_ref()
            && *at == now
        {
            return Ok(Arc::clone(keys));
        }

        let tx = self.db.begin_read().map_err(db_err)?;
        let codes = tx.open_table(CODES).map_err(db_err)?;
        let mut keys = Vec::new();
        for row in codes.iter().map_err(db_err)? {
            keys.push(row.map_err(db_err)?.0.value());
        }
        let keys = Arc::new(keys);
        if let Ok(mut held) = self.keys.write() {
            *held = Some((now, Arc::clone(&keys)));
        }
        Ok(keys)
    }

    /// The total, and the page of it that is held.
    fn remembered(&self, query: &str) -> Option<(usize, Vec<Hit>)> {
        let now = self.generation();
        let mut answers = self.answers.lock().ok()?;
        let at = answers.iter().position(|(at, q, _, _)| *at == now && q == query)?;
        // Asked again, so it is the most recent thing anyone wanted.
        let found = answers.remove(at);
        let answer = (found.2, found.3.clone());
        answers.insert(0, found);
        Some(answer)
    }

    fn remember(&self, query: &str, total: usize, hits: &[Hit]) {
        let now = self.generation();
        let Ok(mut answers) = self.answers.lock() else { return };
        answers.retain(|(at, q, _, _)| *at == now && q != query);
        answers.insert(0, (now, query.to_string(), total, hits.to_vec()));
        answers.truncate(REMEMBERED);
    }

    // --- shards already taken in ---

    /// The published shards this machine has already merged, by file name.
    ///
    /// A shard's name carries the hash of its bytes, so a name that is
    /// already here names a file whose contents are already here. Coming
    /// back for a corpus that has grown then costs only the part that is
    /// new, rather than the four hundred megabytes already on the disk.
    pub fn taken_shards(&self) -> Result<std::collections::HashSet<String>, DialogueError> {
        let Some(raw) = self.setting("taken_shards")? else { return Ok(Default::default()) };
        Ok(serde_json::from_str(&raw).unwrap_or_default())
    }

    pub fn note_shard(&self, name: &str) -> Result<(), DialogueError> {
        let mut taken = self.taken_shards()?;
        if !taken.insert(name.to_string()) {
            return Ok(());
        }
        let json =
            serde_json::to_string(&taken).map_err(|e| DialogueError::Corrupt(e.to_string()))?;
        self.set_setting("taken_shards", &json)
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
        self.changed();
        Ok(())
    }

    /// Where the corpus directory was last imported from, so the
    /// embeddings can be opened without asking again.
    pub fn artifact_dir(&self) -> Result<Option<PathBuf>, DialogueError> {
        Ok(self.setting("artifact_dir")?.map(PathBuf::from))
    }

    pub fn set_artifact_dir(&self, dir: &Path) -> Result<(), DialogueError> {
        self.set_setting("artifact_dir", &dir.display().to_string())
    }

    /// Where to send a phrase to be embedded, if the user set one up.
    pub fn embedder(&self) -> Result<Option<String>, DialogueError> {
        self.setting("embedder")
    }

    pub fn set_embedder(&self, json: &str) -> Result<(), DialogueError> {
        self.set_setting("embedder", json)
    }

    // --- queue ---

    /// Adds galleries to the queue. Finished ones are skipped; a pending one
    /// is promoted if the new priority is higher, never demoted.
    pub fn enqueue(&self, ids: &[i32], priority: Priority) -> Result<usize, DialogueError> {
        self.enqueue_with(ids, priority, false)
    }

    /// `force` re-queues galleries that are already done, so text imported
    /// from elsewhere can be recognised again locally. Their stored text
    /// stays searchable until the new pass replaces it.
    pub fn enqueue_with(
        &self,
        ids: &[i32],
        priority: Priority,
        force: bool,
    ) -> Result<usize, DialogueError> {
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
                    Some(job) if job.status == Status::Done && !force => continue,
                    Some(job) if job.status == Status::Done => {
                        // Done jobs hold no queue key; nothing to remove.
                        let _ = job;
                    }
                    Some(job) if job.status == Status::Pending && job.priority >= priority => {
                        continue;
                    }
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
                    from_reading: false,
                };
                jobs.insert(id, serde_json::to_string(&record)?.as_str()).map_err(db_err)?;
                queue.insert((priority.key(), now, id), ()).map_err(db_err)?;
                added += 1;
            }
        }
        tx.commit().map_err(db_err)?;
        self.changed();
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
        let encoded = Encoded::from_pages(pages);
        let lines: usize = pages.iter().map(|p| p.lines.len()).sum();
        self.finish(
            id,
            Outcome {
                status: Status::Done,
                text: Some(&encoded),
                pages: pages.len() as u32,
                lines: lines as u32,
                error: None,
                source: None,
                from_reading: false,
            },
        )
    }

    pub fn fail(&self, id: i32, error: &str) -> Result<(), DialogueError> {
        self.finish(
            id,
            Outcome {
                status: Status::Failed,
                text: None,
                pages: 0,
                lines: 0,
                error: Some(error),
                source: None,
                from_reading: false,
            },
        )
    }

    fn finish(&self, id: i32, outcome: Outcome<'_>) -> Result<(), DialogueError> {
        let tx = self.db.begin_write().map_err(db_err)?;
        {
            let mut jobs = tx.open_table(JOBS).map_err(db_err)?;
            let mut queue = tx.open_table(QUEUE).map_err(db_err)?;
            let mut texts = tx.open_table(TEXT).map_err(db_err)?;
            let mut codes = tx.open_table(CODES).map_err(db_err)?;
            Self::finish_within(&mut jobs, &mut queue, &mut texts, &mut codes, id, outcome)?;
        }
        tx.commit().map_err(db_err)?;
        self.changed();
        Ok(())
    }

    fn finish_within(
        jobs: &mut redb::Table<'_, i32, &'static str>,
        queue: &mut redb::Table<'_, (u8, u64, i32), ()>,
        texts: &mut redb::Table<'_, i32, &'static [u8]>,
        codes: &mut redb::Table<'_, i32, &'static [u8]>,
        id: i32,
        outcome: Outcome<'_>,
    ) -> Result<(), DialogueError> {
        let Outcome { status, text, pages, lines, error, source, from_reading } = outcome;
        {
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
                from_reading: from_reading || previous.as_ref().is_some_and(|j| j.from_reading),
            };
            jobs.insert(id, serde_json::to_string(&record)?.as_str()).map_err(db_err)?;
            if let Some(encoded) = text {
                texts.insert(id, encoded.text.as_slice()).map_err(db_err)?;
                codes.insert(id, encoded.codes.as_slice()).map_err(db_err)?;
            }
        }
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

    /// Ids this machine recognised itself, as opposed to imported text.
    ///
    /// Imported corpora have gaps: their OCR missed bubbles that Vision
    /// reads. The sweep can be told to go over them again, and this is how
    /// it knows which it has already redone.
    pub fn locally_indexed_ids(&self) -> Result<std::collections::HashSet<i32>, DialogueError> {
        let tx = self.db.begin_read().map_err(db_err)?;
        let jobs = tx.open_table(JOBS).map_err(db_err)?;
        let mut out = std::collections::HashSet::new();
        for row in jobs.iter().map_err(db_err)? {
            let (key, value) = row.map_err(db_err)?;
            let job: JobRecord = serde_json::from_str(value.value())?;
            if job.status == Status::Done && job.source.is_none() {
                out.insert(key.value());
            }
        }
        Ok(out)
    }

    /// Adds pages that are not stored yet, leaving alone any that are.
    ///
    /// A work being read hands its pages over one at a time, so the record
    /// grows instead of being replaced, and text that came from an import is
    /// not overwritten by a second reading of the same page.
    pub fn merge_pages(
        &self,
        id: i32,
        pages: &[PageText],
        from_reading: bool,
    ) -> Result<usize, DialogueError> {
        let mut merged = self.text(id)?.unwrap_or_default();
        let known: std::collections::HashSet<u16> = merged.iter().map(|p| p.page).collect();
        let before = merged.len();
        for page in pages {
            if page.lines.is_empty() || known.contains(&page.page) {
                continue;
            }
            merged.push(page.clone());
        }
        let added = merged.len() - before;
        if added == 0 {
            return Ok(0);
        }
        merged.sort_unstable_by_key(|p| p.page);

        let previous_job = self.job(id)?;
        let source = previous_job.as_ref().and_then(|j| j.source.clone());
        // A work counts as collected by reading only when reading is why it
        // is here at all. Adding a page to text that was imported or swept
        // must not make the whole record something "delete what I read"
        // would throw away.
        let collected_by_reading = from_reading && previous_job.is_none();
        let encoded = Encoded::from_pages(&merged);
        let lines: usize = merged.iter().map(|p| p.lines.len()).sum();
        self.finish(
            id,
            Outcome {
                status: Status::Done,
                text: Some(&encoded),
                pages: merged.len() as u32,
                lines: lines as u32,
                error: None,
                source: source.as_deref(),
                from_reading: collected_by_reading,
            },
        )?;
        Ok(added)
    }

    /// Remembers the embedding of one passage this machine read.
    pub fn put_vector(&self, id: i32, page: u16, vector: &[f32]) -> Result<(), DialogueError> {
        let mut raw = Vec::with_capacity(vector.len() * 4);
        for value in vector {
            raw.extend_from_slice(&value.to_le_bytes());
        }
        let tx = self.db.begin_write().map_err(db_err)?;
        {
            let mut table = tx.open_table(VECTORS).map_err(db_err)?;
            table.insert((id, page), raw.as_slice()).map_err(db_err)?;
        }
        tx.commit().map_err(db_err)?;
        self.changed();
        Ok(())
    }

    pub fn vector(&self, id: i32, page: u16) -> Result<Option<Vec<f32>>, DialogueError> {
        let tx = self.db.begin_read().map_err(db_err)?;
        let table = tx.open_table(VECTORS).map_err(db_err)?;
        Ok(table.get((id, page)).map_err(db_err)?.map(|v| decode_vector(v.value())))
    }

    /// Every passage this machine embedded, for scanning beside the index.
    pub fn each_vector(
        &self,
        mut visit: impl FnMut(i32, u16, &[f32]),
    ) -> Result<usize, DialogueError> {
        let tx = self.db.begin_read().map_err(db_err)?;
        let table = tx.open_table(VECTORS).map_err(db_err)?;
        let mut seen = 0;
        for entry in table.iter().map_err(db_err)? {
            let (key, value) = entry.map_err(db_err)?;
            let (id, page) = key.value();
            visit(id, page, &decode_vector(value.value()));
            seen += 1;
        }
        Ok(seen)
    }

    /// How many passages have an embedding here.
    pub fn vector_count(&self) -> Result<u64, DialogueError> {
        let tx = self.db.begin_read().map_err(db_err)?;
        tx.open_table(VECTORS).map_err(db_err)?.len().map_err(db_err)
    }

    /// Removes everything stored for one gallery.
    pub fn forget(&self, id: i32) -> Result<bool, DialogueError> {
        let tx = self.db.begin_write().map_err(db_err)?;
        let existed = {
            let mut jobs = tx.open_table(JOBS).map_err(db_err)?;
            let mut queue = tx.open_table(QUEUE).map_err(db_err)?;
            let mut texts = tx.open_table(TEXT).map_err(db_err)?;
            let mut codes = tx.open_table(CODES).map_err(db_err)?;
            let previous: Option<JobRecord> = jobs
                .get(id)
                .map_err(db_err)?
                .map(|v| serde_json::from_str(v.value()))
                .transpose()?;
            if let Some(job) = previous.as_ref() {
                queue.remove((job.priority.key(), job.added_at, id)).map_err(db_err)?;
            }
            texts.remove(id).map_err(db_err)?;
            codes.remove(id).map_err(db_err)?;
            jobs.remove(id).map_err(db_err)?;
            // Vectors are keyed per page, so they go by range.
            let mut vectors = tx.open_table(VECTORS).map_err(db_err)?;
            vectors.retain_in((id, 0)..=(id, u16::MAX), |_, _| false).map_err(db_err)?;
            previous.is_some()
        };
        tx.commit().map_err(db_err)?;
        self.changed();
        Ok(existed)
    }

    /// Forgets every gallery whose text was read off pages the user opened.
    pub fn forget_read(&self) -> Result<usize, DialogueError> {
        let ids: Vec<i32> =
            self.stored(true, usize::MAX)?.into_iter().map(|entry| entry.id).collect();
        let mut gone = 0;
        for id in ids {
            if self.forget(id)? {
                gone += 1;
            }
        }
        Ok(gone)
    }

    /// What this machine is keeping, newest first.
    ///
    /// `reading_only` narrows it to what was picked up from pages the user
    /// opened, which is the part worth offering to delete: the rest is either
    /// an import or a sweep the user asked for.
    pub fn stored(&self, reading_only: bool, limit: usize) -> Result<Vec<Stored>, DialogueError> {
        let tx = self.db.begin_read().map_err(db_err)?;
        let jobs = tx.open_table(JOBS).map_err(db_err)?;
        let texts = tx.open_table(TEXT).map_err(db_err)?;
        let codes = tx.open_table(CODES).map_err(db_err)?;

        let mut out = Vec::new();
        for entry in jobs.iter().map_err(db_err)? {
            let (key, value) = entry.map_err(db_err)?;
            let job: JobRecord = serde_json::from_str(value.value())?;
            if job.status != Status::Done || (reading_only && !job.from_reading) {
                continue;
            }
            let id = key.value();
            let bytes = texts.get(id).map_err(db_err)?.map(|v| v.value().len()).unwrap_or(0)
                + codes.get(id).map_err(db_err)?.map(|v| v.value().len()).unwrap_or(0);
            out.push(Stored {
                id,
                pages: job.pages,
                lines: job.lines,
                bytes: bytes as u64,
                finished_at: job.finished_at,
                from_reading: job.from_reading,
                imported: job.source.is_some(),
            });
        }
        out.sort_unstable_by(|a, b| b.finished_at.cmp(&a.finished_at).then(b.id.cmp(&a.id)));
        out.truncate(limit);
        Ok(out)
    }

    pub fn text(&self, id: i32) -> Result<Option<Vec<PageText>>, DialogueError> {
        let tx = self.db.begin_read().map_err(db_err)?;
        let texts = tx.open_table(TEXT).map_err(db_err)?;
        match texts.get(id).map_err(db_err)? {
            Some(v) => Ok(Some(decode_pages(&decompress(v.value())?)?)),
            None => Ok(None),
        }
    }

    /// Stores many finished works in a few transactions. Works already
    /// present are skipped. `progress` is called with the running count.
    pub fn import_works<I>(
        &self,
        works: I,
        source: &str,
        batch: usize,
        mut progress: impl FnMut(usize),
    ) -> Result<ImportSummary, DialogueError>
    where
        I: IntoIterator<Item = (i32, Vec<PageText>)>,
    {
        let existing = self.done_ids()?;
        let mut summary = ImportSummary::default();
        let batch = batch.max(1);
        let mut pending: Vec<(i32, Encoded, u32, u32)> = Vec::with_capacity(batch);

        let flush = |pending: &mut Vec<(i32, Encoded, u32, u32)>| -> Result<(), DialogueError> {
            if pending.is_empty() {
                return Ok(());
            }
            let tx = self.db.begin_write().map_err(db_err)?;
            {
                let mut jobs = tx.open_table(JOBS).map_err(db_err)?;
                let mut queue = tx.open_table(QUEUE).map_err(db_err)?;
                let mut texts = tx.open_table(TEXT).map_err(db_err)?;
                let mut codes = tx.open_table(CODES).map_err(db_err)?;
                for (id, encoded, pages, lines) in pending.drain(..) {
                    Self::finish_within(
                        &mut jobs,
                        &mut queue,
                        &mut texts,
                        &mut codes,
                        id,
                        Outcome {
                            status: Status::Done,
                            text: Some(&encoded),
                            pages,
                            lines,
                            error: None,
                            source: Some(source),
                            from_reading: false,
                        },
                    )?;
                }
            }
            tx.commit().map_err(db_err)?;
            self.changed();
            Ok(())
        };

        for (id, pages) in works {
            summary.galleries += 1;
            if existing.contains(&id) || pages.is_empty() {
                summary.skipped += 1;
                continue;
            }
            let lines: usize = pages.iter().map(|p| p.lines.len()).sum();
            pending.push((id, Encoded::from_pages(&pages), pages.len() as u32, lines as u32));
            summary.added += 1;
            if pending.len() >= batch {
                flush(&mut pending)?;
                progress(summary.galleries);
            }
        }
        flush(&mut pending)?;
        progress(summary.galleries);
        Ok(summary)
    }

    // --- exchange ---

    /// Packs every stored gallery into shards of `range_size` ids.
    ///
    /// `background_only` leaves out galleries that were indexed because the
    /// user asked for them (imported history, hunts): those reveal what the
    /// user read, and a shared file must not.
    /// Packs what this machine recognised into shard files.
    ///
    /// Imported text is left out whatever `background_only` says: a shard is
    /// this machine's own reading, not a copy of a corpus someone else
    /// published.
    pub fn export_shards(
        &self,
        range_size: i32,
        background_only: bool,
    ) -> Result<Vec<Shard>, DialogueError> {
        self.export_shards_with(range_size, background_only, false)
    }

    /// As `export_shards`, but `include_imported` also passes on text that
    /// came from somebody else's corpus.
    ///
    /// Off for the Settings tab, which is one reader sharing their own
    /// reading. On for building the corpus this project publishes, which is
    /// that same corpus being put somewhere a reader can reach it without
    /// being handed a directory to find.
    pub fn export_shards_with(
        &self,
        range_size: i32,
        background_only: bool,
        include_imported: bool,
    ) -> Result<Vec<Shard>, DialogueError> {
        let range_size = range_size.max(1);
        let tx = self.db.begin_read().map_err(db_err)?;
        let texts = tx.open_table(TEXT).map_err(db_err)?;
        let jobs = tx.open_table(JOBS).map_err(db_err)?;

        let mut shards: Vec<Shard> = Vec::new();
        for row in texts.iter().map_err(db_err)? {
            let (key, value) = row.map_err(db_err)?;
            let id = key.value();
            let job: Option<JobRecord> = jobs
                .get(id)
                .map_err(db_err)?
                .map(|v| serde_json::from_str(v.value()))
                .transpose()?;
            // Text that came from someone else's corpus is not passed on by
            // a reader sharing their own reading: a shard says "this machine
            // read these pages", and forwarding an import would be handing on
            // work that is not theirs to hand on.
            if !include_imported && job.as_ref().is_some_and(|j| j.source.is_some()) {
                continue;
            }
            if background_only && job.is_some_and(|j| j.priority != Priority::Background) {
                continue;
            }
            let pages = decode_pages(&decompress(value.value())?)?;
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
            let encoded = Encoded::from_pages(&entry.pages);
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
                    from_reading: false,
                },
            )?;
            summary.added += 1;
        }
        Ok(summary)
    }

    // --- search ---

    /// Scans every stored gallery across all cores. Returns the best page
    /// per gallery, best galleries first, at most `limit`.
    pub fn search(&self, asked: &str, limit: usize) -> Result<Vec<Hit>, DialogueError> {
        Ok(self.search_page(asked, 0, limit)?.1)
    }

    /// One page of what a phrase matches, and how many it matches in all.
    ///
    /// The scan reads the whole store whatever page is asked for, so paging
    /// is only worth having because the answer is held: the first page pays
    /// for the search and the rest are free until something is indexed.
    ///
    /// Beyond `KEPT` there is nothing left to page through, but the total
    /// still says how many there were - a reader who wants those narrows the
    /// phrase rather than pressing on.
    pub fn search_page(
        &self,
        asked: &str,
        offset: usize,
        limit: usize,
    ) -> Result<(usize, Vec<Hit>), DialogueError> {
        let page = |total: usize, kept: Vec<Hit>| {
            (total, kept.into_iter().skip(offset).take(limit).collect::<Vec<_>>())
        };
        let query = Query::new(asked);
        if query.is_empty() || limit == 0 {
            return Ok((0, Vec::new()));
        }
        if let Some((total, kept)) = self.remembered(asked) {
            return Ok(page(total, kept));
        }

        // Galleries cluster in recent ids, so ranges are cut by count, not by
        // id span; otherwise most threads finish early and one does the work.
        let keys = self.search_keys()?;
        if keys.is_empty() {
            return Ok((0, Vec::new()));
        }
        let threads =
            std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 16);
        let per = keys.len().div_ceil(threads);
        let ranges: Vec<(i32, i32)> = keys.chunks(per).map(|c| (c[0], c[c.len() - 1])).collect();

        let scan = |fuzzy: bool| -> Vec<Hit> {
            std::thread::scope(|scope| {
                let mut handles = Vec::with_capacity(ranges.len());
                for &(lo, hi) in &ranges {
                    let query = &query;
                    handles.push(scope.spawn(move || self.scan_range(lo, hi, query, fuzzy)));
                }
                let mut all = Vec::new();
                for handle in handles {
                    match handle.join() {
                        Ok(Ok(part)) => all.extend(part),
                        Ok(Err(err)) => tracing::warn!(%err, "a search thread failed"),
                        Err(_) => tracing::warn!("a search thread panicked"),
                    }
                }
                all
            })
        };

        // Exact hits are cheap to find; only pay for the fuzzy sweep when
        // they cannot fill the page.
        let mut hits = scan(false);
        if hits.len() < offset + limit {
            let exact_ids: std::collections::HashSet<i32> =
                hits.iter().map(|h| h.gallery_id).collect();
            hits.extend(scan(true).into_iter().filter(|h| !exact_ids.contains(&h.gallery_id)));
        }

        hits.sort_by(|a, b| {
            b.exact
                .cmp(&a.exact)
                .then(b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal))
                .then(b.gallery_id.cmp(&a.gallery_id))
        });
        let mut hits = collapse_duplicates(hits);
        let total = hits.len();
        hits.truncate(KEPT);
        self.remember(asked, total, &hits);
        Ok(page(total, hits))
    }

    fn scan_range(
        &self,
        lo: i32,
        hi: i32,
        query: &Query,
        fuzzy: bool,
    ) -> Result<Vec<Hit>, DialogueError> {
        let tx = self.db.begin_read().map_err(db_err)?;
        let codes = tx.open_table(CODES).map_err(db_err)?;
        let texts = tx.open_table(TEXT).map_err(db_err)?;
        let mut hits = Vec::new();
        let mut raw = Vec::new();

        for row in codes.range(lo..=hi).map_err(db_err)? {
            let (key, value) = row.map_err(db_err)?;
            let gallery_id = key.value();
            raw.clear();
            if decompress_into(value.value(), &mut raw).is_err() {
                // Damaged record: skip rather than fail the whole search.
                continue;
            }

            let mut best: Option<(u16, Match)> = None;
            for page in CodeIter::new(&raw) {
                let m = if fuzzy {
                    match query.fuzzy(page.codes) {
                        Some(m) => m,
                        None => continue,
                    }
                } else if query.is_exact(page.codes) {
                    Match { score: 1.0, exact: true }
                } else {
                    continue;
                };
                if best.as_ref().is_none_or(|(_, b)| m.score > b.score) {
                    best = Some((page.page, m));
                }
                if m.exact {
                    break;
                }
            }

            // Only now touch the text: a hit is rare, a page is not.
            if let Some((page_no, m)) = best {
                let lines = texts
                    .get(gallery_id)
                    .map_err(db_err)?
                    .and_then(|v| decompress(v.value()).ok())
                    .and_then(|bytes| {
                        PageIter::new(&bytes)
                            .find(|p| p.page == page_no)
                            .map(|p| p.text.split('\n').map(str::to_string).collect::<Vec<_>>())
                    })
                    .unwrap_or_default();
                hits.push(Hit {
                    gallery_id,
                    page: page_no,
                    score: m.score,
                    exact: m.exact,
                    snippet: snippet(&lines, query),
                    also: Vec::new(),
                });
            }
        }
        Ok(hits)
    }
}

/// Codes only: repeated `u16 page, u32 len, bytes`.
fn decode_vector(raw: &[u8]) -> Vec<f32> {
    raw.as_chunks::<4>().0.iter().copied().map(f32::from_le_bytes).collect()
}

pub fn encode_codes(pages: &[PageText]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut codes = Vec::new();
    for page in pages {
        codes.clear();
        for (i, line) in page.lines.iter().enumerate() {
            if i > 0 {
                codes.push(b' ');
            }
            codes_into(line, &mut codes);
        }
        codes.retain(|&b| b != b' ');
        out.extend_from_slice(&page.page.to_le_bytes());
        out.extend_from_slice(&(codes.len() as u32).to_le_bytes());
        out.extend_from_slice(&codes);
    }
    out
}

struct CodeView<'a> {
    page: u16,
    codes: &'a [u8],
}

struct CodeIter<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> CodeIter<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }
}

impl<'a> Iterator for CodeIter<'a> {
    type Item = CodeView<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let b = self.bytes;
        if self.at + 6 > b.len() {
            return None;
        }
        let page = u16::from_le_bytes([b[self.at], b[self.at + 1]]);
        let len = u32::from_le_bytes(b[self.at + 2..self.at + 6].try_into().ok()?) as usize;
        let start = self.at + 6;
        let end = start.checked_add(len)?;
        let codes = b.get(start..end)?;
        self.at = end;
        Some(CodeView { page, codes })
    }
}

/// Binary page encoding: repeated `u16 page, u32 len, bytes` where the bytes
/// are the page's lines joined with newlines. No JSON on any path.
pub fn encode_pages(pages: &[PageText]) -> Vec<u8> {
    let mut out = Vec::new();
    for page in pages {
        let text = page.lines.join("\n");
        out.extend_from_slice(&page.page.to_le_bytes());
        out.extend_from_slice(&(text.len() as u32).to_le_bytes());
        out.extend_from_slice(text.as_bytes());
    }
    out
}

pub fn decode_pages(bytes: &[u8]) -> Result<Vec<PageText>, DialogueError> {
    let mut pages = Vec::new();
    for page in PageIter::new(bytes) {
        pages.push(PageText {
            page: page.page,
            lines: page.text.split('\n').map(str::to_string).collect(),
        });
    }
    if pages.is_empty() && !bytes.is_empty() {
        return Err(DialogueError::Corrupt("page encoding could not be read".into()));
    }
    Ok(pages)
}

struct PageView<'a> {
    page: u16,
    text: &'a str,
}

/// Walks the binary page encoding without copying the text.
struct PageIter<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> PageIter<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }
}

impl<'a> Iterator for PageIter<'a> {
    type Item = PageView<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let b = self.bytes;
        if self.at + 6 > b.len() {
            return None;
        }
        let page = u16::from_le_bytes([b[self.at], b[self.at + 1]]);
        let len = u32::from_le_bytes(b[self.at + 2..self.at + 6].try_into().ok()?) as usize;
        let start = self.at + 6;
        let end = start.checked_add(len)?;
        let text = std::str::from_utf8(b.get(start..end)?).ok()?;
        self.at = end;
        Some(PageView { page, text })
    }
}

/// Folds copies of the same upload into one result.
///
/// Sorted order puts the newest id first, so that is the one kept and the
/// rest are listed under `also`.
fn collapse_duplicates(hits: Vec<Hit>) -> Vec<Hit> {
    let mut out: Vec<Hit> = Vec::with_capacity(hits.len());
    let mut seen: std::collections::HashMap<(u16, Vec<u8>), usize> =
        std::collections::HashMap::new();
    for hit in hits {
        // Match codes, not raw text: copies differ by OCR noise in the
        // punctuation ("온거야 …?" against "온거야 ?").
        let mut key_codes = Vec::new();
        for line in &hit.snippet {
            codes_into(line, &mut key_codes);
        }
        let key = (hit.page, key_codes);
        match seen.get(&key) {
            Some(&index) => out[index].also.push(hit.gallery_id),
            None => {
                seen.insert(key, out.len());
                out.push(hit);
            }
        }
    }
    out
}

/// The line that matches best plus one line either side.
fn snippet(lines: &[String], query: &Query) -> Vec<String> {
    let mut best_index = 0usize;
    let mut best_score = -1.0f32;
    for i in 0..lines.len() {
        // Score each line together with its neighbours so a phrase that
        // straddles two lines is attributed to the right spot.
        let window: String = lines[i.saturating_sub(1)..(i + 2).min(lines.len())].join("");
        let mut window_codes = Vec::new();
        codes_into(&window, &mut window_codes);
        if let Some(m) = query.score(&window_codes)
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

fn compress(raw: &[u8]) -> Vec<u8> {
    lz4_flex::compress_prepend_size(raw)
}

fn decompress(encoded: &[u8]) -> Result<Vec<u8>, DialogueError> {
    lz4_flex::decompress_size_prepended(encoded).map_err(|e| DialogueError::Corrupt(e.to_string()))
}

/// Decompresses into a reused buffer; the size prefix is read first.
fn decompress_into(encoded: &[u8], out: &mut Vec<u8>) -> Result<(), DialogueError> {
    if encoded.len() < 4 {
        return Err(DialogueError::Corrupt("record too short".into()));
    }
    let size = u32::from_le_bytes(encoded[0..4].try_into().unwrap()) as usize;
    out.resize(size, 0);
    let written = lz4_flex::decompress_into(&encoded[4..], out)
        .map_err(|e| DialogueError::Corrupt(e.to_string()))?;
    out.truncate(written);
    Ok(())
}

fn db_err(err: impl std::fmt::Display) -> DialogueError {
    DialogueError::Database(err.to_string())
}

fn now_millis() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    fn page(n: u16, line: &str) -> PageText {
        PageText { page: n, lines: vec![line.to_string()] }
    }

    #[test]
    fn reading_a_page_adds_it_without_touching_the_rest() {
        let (store, _dir) = store();
        assert_eq!(store.merge_pages(7, &[page(0, "첫 줄")], true).unwrap(), 1);
        assert_eq!(store.merge_pages(7, &[page(2, "셋째 줄")], true).unwrap(), 1);
        // The same page again changes nothing.
        assert_eq!(store.merge_pages(7, &[page(0, "다른 인식")], true).unwrap(), 0);

        let text = store.text(7).unwrap().unwrap();
        assert_eq!(text.iter().map(|p| p.page).collect::<Vec<_>>(), [0, 2]);
        assert_eq!(text[0].lines, ["첫 줄"]);
        assert!(store.job(7).unwrap().unwrap().from_reading);
    }

    #[test]
    fn reading_a_page_of_an_indexed_work_does_not_make_it_deletable_as_read() {
        let (store, _dir) = store();
        store.complete(7, &[page(0, "배경 색인")]).unwrap();
        store.merge_pages(7, &[page(1, "읽으면서")], true).unwrap();

        // The page is kept, but the work still belongs to the sweep: it was
        // not collected by reading, so forgetting what reading collected
        // must not take the swept text with it.
        assert_eq!(store.text(7).unwrap().unwrap().len(), 2);
        assert!(!store.job(7).unwrap().unwrap().from_reading);
        assert_eq!(store.forget_read().unwrap(), 0);
        assert!(store.text(7).unwrap().is_some());
    }

    #[test]
    fn a_passage_embedded_here_is_kept_and_scanned() {
        let (store, _dir) = store();
        store.merge_pages(7, &[page(0, "좋아해")], true).unwrap();
        store.put_vector(7, 0, &[0.5, 0.5, 0.5, 0.5]).unwrap();
        store.put_vector(8, 2, &[1.0, 0.0, 0.0, 0.0]).unwrap();

        assert_eq!(store.vector(7, 0).unwrap(), Some(vec![0.5, 0.5, 0.5, 0.5]));
        assert_eq!(store.vector(7, 1).unwrap(), None);
        assert_eq!(store.vector_count().unwrap(), 2);

        let mut seen = Vec::new();
        store.each_vector(|id, page, v| seen.push((id, page, v.len()))).unwrap();
        assert_eq!(seen, [(7, 0, 4), (8, 2, 4)]);
    }

    #[test]
    fn forgetting_a_gallery_takes_its_vectors_with_it() {
        let (store, _dir) = store();
        store.merge_pages(7, &[page(0, "좋아해")], true).unwrap();
        store.put_vector(7, 0, &[1.0, 0.0]).unwrap();
        store.put_vector(7, 1, &[0.0, 1.0]).unwrap();
        store.put_vector(9, 0, &[1.0, 1.0]).unwrap();

        store.forget(7).unwrap();
        assert_eq!(store.vector(7, 0).unwrap(), None);
        assert_eq!(store.vector(7, 1).unwrap(), None);
        // A different work is untouched.
        assert_eq!(store.vector(9, 0).unwrap(), Some(vec![1.0, 1.0]));
    }

    #[test]
    fn an_imported_corpus_is_never_exported() {
        let (store, _dir) = store();
        store.complete(1, &[page(0, "이 기계가 읽은 것")]).unwrap();
        store.import_works(vec![(2, pages(&[&["남의 코퍼스"]]))], "artifact", 10, |_| {}).unwrap();

        for background_only in [true, false] {
            let shards = store.export_shards(1_000_000, background_only).unwrap();
            let ids: Vec<i32> =
                shards.iter().flat_map(|s| s.entries.iter().map(|e| e.gallery_id)).collect();
            assert_eq!(ids, [1], "background_only={background_only}");
        }
    }

    #[test]
    fn forgetting_a_gallery_leaves_nothing_behind() {
        let (store, _dir) = store();
        store.merge_pages(7, &[page(0, "좋아해")], true).unwrap();
        assert!(!store.search("좋아해", 5).unwrap().is_empty());

        assert!(store.forget(7).unwrap());
        assert_eq!(store.text(7).unwrap(), None);
        assert!(store.job(7).unwrap().is_none());
        assert!(store.search("좋아해", 5).unwrap().is_empty());
        // Forgetting what is not there is not an error.
        assert!(!store.forget(7).unwrap());
    }

    #[test]
    fn only_what_reading_produced_is_offered_for_deletion() {
        let (store, _dir) = store();
        store.complete(1, &[page(0, "배경")]).unwrap();
        store.merge_pages(2, &[page(0, "읽은 것")], true).unwrap();
        store.merge_pages(3, &[page(0, "읽은 것")], true).unwrap();

        let all = store.stored(false, 10).unwrap();
        assert_eq!(all.len(), 3);
        let read = store.stored(true, 10).unwrap();
        assert_eq!(read.iter().map(|s| s.id).collect::<Vec<_>>(), [3, 2]);
        assert!(read[0].bytes > 0);

        assert_eq!(store.forget_read().unwrap(), 2);
        assert_eq!(store.stored(false, 10).unwrap().len(), 1);
        assert_eq!(store.stored(false, 10).unwrap()[0].id, 1);
    }

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
    fn forcing_requeues_a_done_gallery_but_keeps_its_text() {
        let (store, _d) = store();
        store.enqueue(&[9], Priority::Background).unwrap();
        store.complete(9, &pages(&[&["원래 텍스트"]])).unwrap();

        assert_eq!(store.enqueue(&[9], Priority::Hunt).unwrap(), 0);
        assert_eq!(store.enqueue_with(&[9], Priority::Hunt, true).unwrap(), 1);
        assert_eq!(store.next_pending().unwrap(), Some(9));
        assert_eq!(store.text(9).unwrap().unwrap()[0].lines, vec!["원래 텍스트"]);

        store.complete(9, &pages(&[&["다시 읽은 텍스트"]])).unwrap();
        assert_eq!(store.text(9).unwrap().unwrap()[0].lines, vec!["다시 읽은 텍스트"]);
    }

    #[test]
    fn locally_indexed_excludes_imported_text() {
        let (store, _d) = store();
        store.enqueue(&[1], Priority::Background).unwrap();
        store.complete(1, &pages(&[&["mine"]])).unwrap();
        store.import_works(vec![(2, pages(&[&["theirs"]]))], "artifact", 10, |_| {}).unwrap();
        assert_eq!(store.locally_indexed_ids().unwrap(), [1].into_iter().collect());
        assert_eq!(store.done_ids().unwrap().len(), 2);
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
        store
            .complete(3, &pages(&[&["뭐라고?"], &["일단 구급차라도", "부르는 게 좋겠어요."]]))
            .unwrap();

        let hits = store.search("구급차라도 부르는 게 좋겠어요", 10).unwrap();
        assert_eq!(hits.iter().map(|h| h.gallery_id).collect::<Vec<_>>(), vec![3, 2]);
        assert!(hits[0].exact);
        assert_eq!(hits[0].page, 1);
        assert!(!hits[1].exact);
        assert!(hits[0].snippet.iter().any(|l| l.contains("구급차")));
    }

    #[test]
    fn copies_of_the_same_upload_collapse_into_one_result() {
        let (store, _d) = store();
        // hitomi carries the same work under several ids; every copy matches.
        store.enqueue(&[100, 101, 200], Priority::Background).unwrap();
        for id in [100, 101] {
            store.complete(id, &pages(&[&["구급차라도", "부르는 게 좋겠어요"]])).unwrap();
        }
        store.complete(200, &pages(&[&["다른 작품", "구급차라도 부르는 게 좋겠어요"]])).unwrap();

        let hits = store.search("구급차라도 부르는 게", 10).unwrap();
        assert_eq!(hits.len(), 2, "two distinct works, not three rows");
        let collapsed = hits.iter().find(|h| h.gallery_id == 101).unwrap();
        assert_eq!(collapsed.also, vec![100], "the older copy is listed, not dropped");
    }

    #[test]
    fn copies_collapse_through_ocr_noise_in_punctuation() {
        let (store, _d) = store();
        store.enqueue(&[300, 301], Priority::Background).unwrap();
        store.complete(300, &pages(&[&["도와주러 온거야 …?", "구급차라도 부를까요~?"]])).unwrap();
        store.complete(301, &pages(&[&["도와주러 온거야 ?", "구급차라도 부를까요~?"]])).unwrap();

        let hits = store.search("구급차라도 부를까요", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].also, vec![300]);
    }

    /// A second ask for the same thing is answered from what was kept.
    #[test]
    fn the_same_question_is_not_asked_of_the_disk_twice() {
        let dir = tempfile::tempdir().unwrap();
        let store = DialogueStore::open(dir.path().join("d.redb")).unwrap();
        store.complete(1, &[page(0, "구급차라도 부르는 게 좋겠어요")]).unwrap();

        let first = store.search("구급차", 5).unwrap();
        assert_eq!(first.len(), 1);
        let again = store.search("구급차", 5).unwrap();
        assert_eq!(again.len(), first.len());
        assert_eq!(again[0].gallery_id, first[0].gallery_id);
        assert_eq!(again[0].snippet, first[0].snippet);
        // Kept by the phrase, not by the page size: a second page of the same
        // question is answered without touching the disk again.
        assert!(store.remembered("구급차").is_some());
        assert!(store.remembered("소방차").is_none());
    }

    /// A shard taken in once is not fetched again.
    #[test]
    fn what_has_been_taken_in_is_remembered() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("d.redb");
        {
            let store = DialogueStore::open(&path).unwrap();
            assert!(store.taken_shards().unwrap().is_empty());
            store.note_shard("dialogue-0-99999-0dfcae8415594e82.tsd").unwrap();
            store.note_shard("dialogue-0-99999-0dfcae8415594e82.tsd").unwrap();
            store.note_shard("dialogue-100000-199999-c8d05fd500443748.tsd").unwrap();
        }
        let store = DialogueStore::open(&path).unwrap();
        let taken = store.taken_shards().unwrap();
        assert_eq!(taken.len(), 2, "the same shard twice is one shard");
        assert!(taken.contains("dialogue-0-99999-0dfcae8415594e82.tsd"));
    }

    /// The count is of everything that matched, not of the page handed back.
    #[test]
    fn a_page_says_how_many_there_were_in_all() {
        let (store, _d) = store();
        // Distinct around the phrase, or they would fold together as copies
        // of one another - which is what a wall of re-uploads deserves, and
        // not what is being counted here.
        for id in 1..=7 {
            store.complete(id, &[page(0, &format!("안녕하세요 {id}번 손님"))]).unwrap();
        }
        let (total, first) = store.search_page("안녕하세요", 0, 3).unwrap();
        assert_eq!(total, 7, "seven works said it");
        assert_eq!(first.len(), 3);

        let (again, second) = store.search_page("안녕하세요", 3, 3).unwrap();
        assert_eq!(again, 7);
        assert_eq!(second.len(), 3);
        let seen: std::collections::HashSet<i32> =
            first.iter().chain(second.iter()).map(|h| h.gallery_id).collect();
        assert_eq!(seen.len(), 6, "the second page is not the first one again");

        let (_, last) = store.search_page("안녕하세요", 6, 3).unwrap();
        assert_eq!(last.len(), 1);
        assert!(store.search_page("안녕하세요", 99, 3).unwrap().1.is_empty());
    }

    /// Storing anything makes what was kept stale, answers and keys alike.
    #[test]
    fn what_was_kept_does_not_survive_a_write() {
        let dir = tempfile::tempdir().unwrap();
        let store = DialogueStore::open(dir.path().join("d.redb")).unwrap();
        store.complete(1, &[page(0, "구급차라도 부르는 게 좋겠어요")]).unwrap();
        assert_eq!(store.search("구급차", 5).unwrap().len(), 1);
        assert!(store.remembered("구급차").is_some());
        let keys = store.search_keys().unwrap();
        assert_eq!(keys.len(), 1);

        store.complete(2, &[page(0, "구급차를 불렀다")]).unwrap();
        assert!(store.remembered("구급차").is_none(), "the old answer is stale");
        assert_eq!(store.search_keys().unwrap().len(), 2, "the new work is searchable");
        assert_eq!(store.search("구급차", 5).unwrap().len(), 2);
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
        let all: usize =
            store.export_shards(1_000, false).unwrap().iter().map(|s| s.entries.len()).sum();
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
    fn page_encoding_round_trips_without_json() {
        let p = pages(&[&["일단", "구급차라도"], &["좋겠어요"]]);
        let bytes = encode_pages(&p);
        assert_eq!(decode_pages(&bytes).unwrap(), p);
        assert!(decode_pages(&bytes[..3]).is_err());

        let codes = encode_codes(&p);
        let views: Vec<(u16, usize)> =
            CodeIter::new(&codes).map(|c| (c.page, c.codes.len())).collect();
        assert_eq!(views.len(), 2);
        assert_eq!(views[0].0, 0);
        assert!(views[0].1 > 0);
    }

    #[test]
    fn bulk_import_batches_and_skips_existing() {
        let (store, _d) = store();
        store.enqueue(&[2], Priority::Background).unwrap();
        store.complete(2, &pages(&[&["already here"]])).unwrap();
        let works = (1..=5).map(|id| (id, pages(&[&["대사"]])));
        let mut calls = 0;
        let summary = store.import_works(works, "artifact", 2, |_| calls += 1).unwrap();
        assert_eq!((summary.galleries, summary.added, summary.skipped), (5, 4, 1));
        assert!(calls >= 2, "progress is reported per batch");
        assert_eq!(store.text(2).unwrap().unwrap()[0].lines, vec!["already here"]);
        assert_eq!(store.job(5).unwrap().unwrap().source.as_deref(), Some("artifact"));
        assert_eq!(store.counts().unwrap().done, 5);
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
