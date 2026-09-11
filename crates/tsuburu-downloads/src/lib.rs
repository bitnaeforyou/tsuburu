//! Pages kept on disk, so a work can be read without hitomi.
//!
//! Images are filed by their hash rather than by gallery and page, which is
//! how the image proxy addresses them: a downloaded page is served from disk
//! by the same URL that would otherwise be relayed, so reading offline needs
//! no separate code path. It also means the copies of a work that hitomi
//! carries under different ids share one file.
//!
//! A record per gallery holds what the reader needs when the network is not
//! there: the page list, their hashes and sizes, and the title.

use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const GALLERIES: TableDefinition<i32, &str> = TableDefinition::new("galleries");
const META: TableDefinition<&str, &str> = TableDefinition::new("meta");
const SCHEMA_VERSION: &str = "1";

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("could not open the downloads database: {0}")]
    Open(String),
    #[error("downloads database error: {0}")]
    Database(String),
    #[error("stored record could not be read: {0}")]
    Corrupt(#[from] serde_json::Error),
    #[error("could not write {0}: {1}")]
    Io(PathBuf, String),
    #[error("this download store was written by a newer tsuburu (schema {found})")]
    NewerSchema { found: String },
    #[error("{0} is not a 64-character hex hash")]
    BadHash(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadedPage {
    /// 0-based, as everywhere else in tsuburu.
    pub page: u16,
    pub hash: String,
    /// `avif` or `webp`.
    pub ext: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Download {
    pub id: i32,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    /// Every page of the work, whether or not it is on disk yet.
    pub pages: Vec<DownloadedPage>,
    pub added_at: u64,
}

/// What a gallery has on disk right now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Progress {
    pub id: i32,
    pub title: Option<String>,
    pub language: Option<String>,
    pub pages: usize,
    pub have: usize,
    pub bytes: u64,
    pub added_at: u64,
}

impl Progress {
    pub fn complete(&self) -> bool {
        self.pages > 0 && self.have == self.pages
    }
}

pub struct DownloadStore {
    db: Database,
    images: PathBuf,
}

impl DownloadStore {
    /// `dir` holds the database and an `images` directory beside it.
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, DownloadError> {
        let dir = dir.as_ref().to_path_buf();
        let images = dir.join("images");
        std::fs::create_dir_all(&images)
            .map_err(|e| DownloadError::Io(images.clone(), e.to_string()))?;
        let db = Database::create(dir.join("downloads.redb"))
            .map_err(|e| DownloadError::Open(e.to_string()))?;
        let store = Self { db, images };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<(), DownloadError> {
        let tx = self.db.begin_write().map_err(db_err)?;
        {
            tx.open_table(GALLERIES).map_err(db_err)?;
            let mut meta = tx.open_table(META).map_err(db_err)?;
            let existing = meta.get("schema").map_err(db_err)?.map(|v| v.value().to_string());
            match existing {
                None => {
                    meta.insert("schema", SCHEMA_VERSION).map_err(db_err)?;
                }
                Some(found) if found == SCHEMA_VERSION => {}
                Some(found) => return Err(DownloadError::NewerSchema { found }),
            }
        }
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    pub fn images_dir(&self) -> &Path {
        &self.images
    }

    /// Where a page lives once downloaded.
    ///
    /// The hash is checked rather than trusted: it reaches this from a URL.
    pub fn image_path(&self, hash: &str, ext: &str) -> Result<PathBuf, DownloadError> {
        if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(DownloadError::BadHash(hash.to_string()));
        }
        let ext = match ext {
            "avif" | "webp" => ext,
            other => return Err(DownloadError::BadHash(other.to_string())),
        };
        // Two levels of fan-out; a flat directory of 100,000 files is slow
        // to list on every platform that matters.
        Ok(self.images.join(&hash[0..2]).join(format!("{hash}.{ext}")))
    }

    pub fn has_image(&self, hash: &str, ext: &str) -> bool {
        self.image_path(hash, ext).is_ok_and(|p| p.is_file())
    }

    pub fn save_image(&self, hash: &str, ext: &str, bytes: &[u8]) -> Result<(), DownloadError> {
        let path = self.image_path(hash, ext)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| DownloadError::Io(parent.to_path_buf(), e.to_string()))?;
        }
        // Write beside the target and rename, so a crash cannot leave a
        // half-written page that later reads as a corrupt image.
        let temporary = path.with_extension(format!("{ext}.part"));
        std::fs::write(&temporary, bytes)
            .map_err(|e| DownloadError::Io(temporary.clone(), e.to_string()))?;
        std::fs::rename(&temporary, &path)
            .map_err(|e| DownloadError::Io(path.clone(), e.to_string()))?;
        Ok(())
    }

    pub fn read_image(&self, hash: &str, ext: &str) -> Result<Option<Vec<u8>>, DownloadError> {
        let path = self.image_path(hash, ext)?;
        match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(DownloadError::Io(path, e.to_string())),
        }
    }

    pub fn put(&self, download: &Download) -> Result<(), DownloadError> {
        let tx = self.db.begin_write().map_err(db_err)?;
        {
            let mut galleries = tx.open_table(GALLERIES).map_err(db_err)?;
            galleries
                .insert(download.id, serde_json::to_string(download)?.as_str())
                .map_err(db_err)?;
        }
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    pub fn get(&self, id: i32) -> Result<Option<Download>, DownloadError> {
        let tx = self.db.begin_read().map_err(db_err)?;
        let galleries = tx.open_table(GALLERIES).map_err(db_err)?;
        match galleries.get(id).map_err(db_err)? {
            Some(v) => Ok(Some(serde_json::from_str(v.value())?)),
            None => Ok(None),
        }
    }

    /// Forgets a gallery and deletes any page no other gallery still wants.
    pub fn remove(&self, id: i32) -> Result<bool, DownloadError> {
        let Some(download) = self.get(id)? else { return Ok(false) };
        let mut wanted: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        for other in self.list_records()? {
            if other.id == id {
                continue;
            }
            for page in other.pages {
                wanted.insert((page.hash, page.ext));
            }
        }

        let tx = self.db.begin_write().map_err(db_err)?;
        {
            let mut galleries = tx.open_table(GALLERIES).map_err(db_err)?;
            galleries.remove(id).map_err(db_err)?;
        }
        tx.commit().map_err(db_err)?;

        for page in download.pages {
            if wanted.contains(&(page.hash.clone(), page.ext.clone())) {
                continue;
            }
            if let Ok(path) = self.image_path(&page.hash, &page.ext) {
                let _ = std::fs::remove_file(path);
            }
        }
        Ok(true)
    }

    fn list_records(&self) -> Result<Vec<Download>, DownloadError> {
        let tx = self.db.begin_read().map_err(db_err)?;
        let galleries = tx.open_table(GALLERIES).map_err(db_err)?;
        let mut out = Vec::new();
        for row in galleries.iter().map_err(db_err)? {
            let (_, value) = row.map_err(db_err)?;
            match serde_json::from_str(value.value()) {
                Ok(download) => out.push(download),
                Err(err) => tracing::warn!(%err, "skipping an unreadable download record"),
            }
        }
        Ok(out)
    }

    pub fn progress(&self, id: i32) -> Result<Option<Progress>, DownloadError> {
        Ok(self.get(id)?.map(|d| self.measure(&d)))
    }

    /// Most recently added first.
    pub fn list(&self) -> Result<Vec<Progress>, DownloadError> {
        let mut out: Vec<Progress> = self.list_records()?.iter().map(|d| self.measure(d)).collect();
        out.sort_unstable_by_key(|p| std::cmp::Reverse(p.added_at));
        Ok(out)
    }

    fn measure(&self, download: &Download) -> Progress {
        let mut have = 0usize;
        let mut bytes = 0u64;
        for page in &download.pages {
            if let Ok(path) = self.image_path(&page.hash, &page.ext)
                && let Ok(meta) = std::fs::metadata(&path)
            {
                have += 1;
                bytes += meta.len();
            }
        }
        Progress {
            id: download.id,
            title: download.title.clone(),
            language: download.language.clone(),
            pages: download.pages.len(),
            have,
            bytes,
            added_at: download.added_at,
        }
    }
}

fn db_err(err: impl std::fmt::Display) -> DownloadError {
    DownloadError::Database(err.to_string())
}

pub fn now_millis() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (DownloadStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        (DownloadStore::open(dir.path()).unwrap(), dir)
    }

    fn hash(seed: char) -> String {
        std::iter::repeat_n(seed, 64).collect()
    }

    fn download(id: i32, hashes: &[char]) -> Download {
        Download {
            id,
            title: Some(format!("Work {id}")),
            language: Some("korean".into()),
            kind: Some("manga".into()),
            pages: hashes
                .iter()
                .enumerate()
                .map(|(i, &h)| DownloadedPage {
                    page: i as u16,
                    hash: hash(h),
                    ext: "avif".into(),
                    width: 100,
                    height: 200,
                })
                .collect(),
            added_at: now_millis(),
        }
    }

    #[test]
    fn images_are_addressed_by_hash_and_written_atomically() {
        let (store, _d) = store();
        let h = hash('a');
        assert!(!store.has_image(&h, "avif"));
        store.save_image(&h, "avif", b"bytes").unwrap();
        assert!(store.has_image(&h, "avif"));
        assert_eq!(store.read_image(&h, "avif").unwrap().unwrap(), b"bytes");
        assert_eq!(store.read_image(&hash('b'), "avif").unwrap(), None);

        let path = store.image_path(&h, "avif").unwrap();
        assert!(path.starts_with(store.images_dir().join("aa")), "fanned out by prefix");
        assert!(!path.with_extension("avif.part").exists(), "no leftovers");
    }

    #[test]
    fn a_bad_hash_or_format_is_refused() {
        let (store, _d) = store();
        // The hash arrives in a URL, so a path that climbs out of the store
        // must be rejected before it reaches the filesystem. Spelled without
        // naming a real system file: secret scanners read the fixture, not
        // the assertion around it.
        for climbing in ["../../elsewhere", "..", "a/../../b", "/absolute"] {
            assert!(
                matches!(store.image_path(climbing, "avif"), Err(DownloadError::BadHash(_))),
                "{climbing} should not resolve"
            );
        }
        assert!(matches!(store.image_path(&hash('a'), "png"), Err(DownloadError::BadHash(_))));
    }

    #[test]
    fn progress_counts_what_is_actually_on_disk() {
        let (store, _d) = store();
        let d = download(1, &['a', 'b', 'c']);
        store.put(&d).unwrap();
        let p = store.progress(1).unwrap().unwrap();
        assert_eq!((p.pages, p.have, p.bytes), (3, 0, 0));
        assert!(!p.complete());

        store.save_image(&hash('a'), "avif", b"12345").unwrap();
        store.save_image(&hash('b'), "avif", b"123").unwrap();
        let p = store.progress(1).unwrap().unwrap();
        assert_eq!((p.have, p.bytes), (2, 8));

        store.save_image(&hash('c'), "avif", b"1").unwrap();
        assert!(store.progress(1).unwrap().unwrap().complete());
    }

    #[test]
    fn removing_a_gallery_keeps_pages_another_one_shares() {
        let (store, _d) = store();
        // hitomi carries the same work twice; the copies share pages.
        store.put(&download(1, &['a', 'b'])).unwrap();
        store.put(&download(2, &['b', 'c'])).unwrap();
        for h in ['a', 'b', 'c'] {
            store.save_image(&hash(h), "avif", b"x").unwrap();
        }

        assert!(store.remove(1).unwrap());
        assert!(!store.has_image(&hash('a'), "avif"), "only gallery 1 wanted it");
        assert!(store.has_image(&hash('b'), "avif"), "gallery 2 still wants it");
        assert!(store.has_image(&hash('c'), "avif"));
        assert!(!store.remove(1).unwrap(), "removing twice is not an error");
    }

    #[test]
    fn listing_is_newest_first_and_survives_reopening() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = DownloadStore::open(dir.path()).unwrap();
            let mut older = download(1, &['a']);
            older.added_at = 1000;
            let mut newer = download(2, &['b']);
            newer.added_at = 2000;
            store.put(&older).unwrap();
            store.put(&newer).unwrap();
        }
        let store = DownloadStore::open(dir.path()).unwrap();
        assert_eq!(store.list().unwrap().iter().map(|p| p.id).collect::<Vec<_>>(), vec![2, 1]);
        assert_eq!(store.get(1).unwrap().unwrap().title.as_deref(), Some("Work 1"));
    }
}
