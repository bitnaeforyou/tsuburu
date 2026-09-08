//! 로컬 라이브러리 — 즐겨찾기, 읽음 기록, 이어보기.
//!
//! redb 파일 하나에 담는다. 순수 Rust라 C 의존성이 없고, 단일 바이너리 배포와
//! 크로스컴파일이 조건 없이 성립한다.
//!
//! 즐겨찾기와 기록은 **카드 요약을 함께 저장한다.** 목록을 그리려고 갤러리
//! 메타데이터를 다시 받으면 한 건에 최대 230 KB가 오간다. 요약을 들고 있으면
//! 목록이 즉시 뜨고 오프라인에서도 보인다.

use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const FAVORITES: TableDefinition<i32, &str> = TableDefinition::new("favorites");
const HISTORY: TableDefinition<i32, &str> = TableDefinition::new("history");
const META: TableDefinition<&str, &str> = TableDefinition::new("meta");

const SCHEMA_VERSION: &str = "1";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("could not open the library database: {0}")]
    Open(String),
    #[error("library database error: {0}")]
    Database(String),
    #[error("stored record could not be read: {0}")]
    Corrupt(#[from] serde_json::Error),
    #[error("no suitable data directory for this platform")]
    NoDataDir,
    #[error(
        "this library was written by a newer version of tsuburu (schema {found}, expected {expected})"
    )]
    NewerSchema { found: String, expected: String },
}

/// 갤러리 목록 한 줄을 그리는 데 필요한 최소 정보.
///
/// 서버의 카드 응답과 같은 모양이라 프론트가 목록 종류를 구분하지 않아도 된다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    pub id: i32,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub pages: usize,
    /// 썸네일 해시. URL이 아니라 해시를 저장한다. 이미지 경로 규칙이 바뀌어도
    /// 저장된 기록이 죽지 않는다.
    #[serde(default)]
    pub thumbnail_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Favorite {
    #[serde(flatten)]
    pub summary: Summary,
    pub added_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    #[serde(flatten)]
    pub summary: Summary,
    pub last_seen_at: u64,
    /// 0부터 센다.
    pub last_page: usize,
}

pub struct Store {
    db: Database,
    path: PathBuf,
}

/// The per-user directory every tsuburu database lives in.
pub fn data_dir() -> Result<PathBuf, StoreError> {
    let dirs =
        directories::ProjectDirs::from("la", "tsuburu", "tsuburu").ok_or(StoreError::NoDataDir)?;
    let dir = dirs.data_dir().to_path_buf();
    std::fs::create_dir_all(&dir).map_err(|e| StoreError::Open(e.to_string()))?;
    Ok(dir)
}

impl Store {
    /// OS 표준 애플리케이션 데이터 디렉터리에 연다.
    pub fn open_default() -> Result<Self, StoreError> {
        Self::open(data_dir()?.join("library.redb"))
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        let db = Database::create(&path).map_err(|e| StoreError::Open(e.to_string()))?;
        let store = Self { db, path };
        store.init_schema()?;
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 테이블을 만들고 스키마 버전을 확인한다.
    ///
    /// 더 새로운 버전이 쓴 파일을 만나면 열지 않는다. 모르는 형식을 덮어써서
    /// 사용자의 즐겨찾기를 날리는 것보다 열지 못하는 편이 낫다.
    fn init_schema(&self) -> Result<(), StoreError> {
        let tx = self.db.begin_write().map_err(db_err)?;
        {
            tx.open_table(FAVORITES).map_err(db_err)?;
            tx.open_table(HISTORY).map_err(db_err)?;
            let mut meta = tx.open_table(META).map_err(db_err)?;
            let existing = meta.get("schema").map_err(db_err)?.map(|v| v.value().to_string());
            match existing {
                None => {
                    meta.insert("schema", SCHEMA_VERSION).map_err(db_err)?;
                }
                Some(found) if found == SCHEMA_VERSION => {}
                Some(found) => {
                    return Err(StoreError::NewerSchema { found, expected: SCHEMA_VERSION.into() });
                }
            }
        }
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    // --- 즐겨찾기 ---

    pub fn add_favorite(&self, summary: Summary) -> Result<Favorite, StoreError> {
        // 이미 있으면 추가 시각을 유지한다. 다시 누른다고 목록 맨 위로 튀어
        // 오르면 사용자가 정리해둔 순서가 흐트러진다.
        let added_at = match self.favorite(summary.id)? {
            Some(existing) => existing.added_at,
            None => now_millis(),
        };
        let favorite = Favorite { summary, added_at };
        self.put(FAVORITES, favorite.summary.id, &favorite)?;
        Ok(favorite)
    }

    pub fn remove_favorite(&self, id: i32) -> Result<bool, StoreError> {
        self.delete(FAVORITES, id)
    }

    pub fn favorite(&self, id: i32) -> Result<Option<Favorite>, StoreError> {
        self.get(FAVORITES, id)
    }

    /// 최근에 추가한 것부터.
    pub fn favorites(&self) -> Result<Vec<Favorite>, StoreError> {
        let mut all: Vec<Favorite> = self.list(FAVORITES)?;
        all.sort_unstable_by_key(|f| std::cmp::Reverse(f.added_at));
        Ok(all)
    }

    // --- 읽음 기록 ---

    /// 진행 상황을 기록한다. 같은 갤러리를 다시 열면 갱신된다.
    pub fn record_progress(
        &self,
        summary: Summary,
        last_page: usize,
    ) -> Result<HistoryEntry, StoreError> {
        let entry = HistoryEntry { summary, last_seen_at: now_millis(), last_page };
        self.put(HISTORY, entry.summary.id, &entry)?;
        Ok(entry)
    }

    pub fn history_entry(&self, id: i32) -> Result<Option<HistoryEntry>, StoreError> {
        self.get(HISTORY, id)
    }

    /// 최근에 본 것부터.
    pub fn history(&self) -> Result<Vec<HistoryEntry>, StoreError> {
        let mut all: Vec<HistoryEntry> = self.list(HISTORY)?;
        all.sort_unstable_by_key(|h| std::cmp::Reverse(h.last_seen_at));
        Ok(all)
    }

    pub fn clear_history(&self) -> Result<(), StoreError> {
        let tx = self.db.begin_write().map_err(db_err)?;
        {
            let mut table = tx.open_table(HISTORY).map_err(db_err)?;
            table.retain(|_, _| false).map_err(db_err)?;
        }
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    // --- 공통 ---

    fn put<T: Serialize>(
        &self,
        table: TableDefinition<i32, &'static str>,
        id: i32,
        value: &T,
    ) -> Result<(), StoreError> {
        let json = serde_json::to_string(value)?;
        let tx = self.db.begin_write().map_err(db_err)?;
        {
            let mut t = tx.open_table(table).map_err(db_err)?;
            t.insert(id, json.as_str()).map_err(db_err)?;
        }
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    fn get<T: for<'de> Deserialize<'de>>(
        &self,
        table: TableDefinition<i32, &'static str>,
        id: i32,
    ) -> Result<Option<T>, StoreError> {
        let tx = self.db.begin_read().map_err(db_err)?;
        let t = tx.open_table(table).map_err(db_err)?;
        match t.get(id).map_err(db_err)? {
            Some(v) => Ok(Some(serde_json::from_str(v.value())?)),
            None => Ok(None),
        }
    }

    fn delete(
        &self,
        table: TableDefinition<i32, &'static str>,
        id: i32,
    ) -> Result<bool, StoreError> {
        let tx = self.db.begin_write().map_err(db_err)?;
        let existed;
        {
            let mut t = tx.open_table(table).map_err(db_err)?;
            existed = t.remove(id).map_err(db_err)?.is_some();
        }
        tx.commit().map_err(db_err)?;
        Ok(existed)
    }

    fn list<T: for<'de> Deserialize<'de>>(
        &self,
        table: TableDefinition<i32, &'static str>,
    ) -> Result<Vec<T>, StoreError> {
        let tx = self.db.begin_read().map_err(db_err)?;
        let t = tx.open_table(table).map_err(db_err)?;
        let mut out = Vec::new();
        for row in t.iter().map_err(db_err)? {
            let (_, value) = row.map_err(db_err)?;
            // 한 줄이 깨졌다고 목록 전체를 버리지 않는다.
            match serde_json::from_str(value.value()) {
                Ok(parsed) => out.push(parsed),
                Err(err) => tracing::warn!(%err, "skipping an unreadable library record"),
            }
        }
        Ok(out)
    }
}

fn db_err(err: impl std::fmt::Display) -> StoreError {
    StoreError::Database(err.to_string())
}

fn now_millis() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (Store, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("test.redb")).unwrap();
        (store, dir)
    }

    fn summary(id: i32) -> Summary {
        Summary {
            id,
            title: Some(format!("Gallery {id}")),
            language: Some("japanese".into()),
            kind: Some("doujinshi".into()),
            pages: 20,
            thumbnail_hash: Some("a".repeat(64)),
        }
    }

    #[test]
    fn favorites_round_trip() {
        let (store, _dir) = store();
        assert!(store.favorites().unwrap().is_empty());

        store.add_favorite(summary(1)).unwrap();
        let found = store.favorite(1).unwrap().unwrap();
        assert_eq!(found.summary.title.as_deref(), Some("Gallery 1"));
        assert_eq!(found.summary.pages, 20);

        assert!(store.remove_favorite(1).unwrap());
        assert!(store.favorite(1).unwrap().is_none());
        assert!(!store.remove_favorite(1).unwrap(), "removing twice is not an error");
    }

    #[test]
    fn favorites_are_listed_newest_first() {
        let (store, _dir) = store();
        for id in [1, 2, 3] {
            store.add_favorite(summary(id)).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let ids: Vec<i32> = store.favorites().unwrap().iter().map(|f| f.summary.id).collect();
        assert_eq!(ids, vec![3, 2, 1]);
    }

    #[test]
    fn re_adding_a_favorite_keeps_its_original_position() {
        let (store, _dir) = store();
        let first = store.add_favorite(summary(1)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        store.add_favorite(summary(2)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));

        let again = store.add_favorite(summary(1)).unwrap();
        assert_eq!(again.added_at, first.added_at, "re-adding must not reorder");

        let ids: Vec<i32> = store.favorites().unwrap().iter().map(|f| f.summary.id).collect();
        assert_eq!(ids, vec![2, 1]);
    }

    #[test]
    fn progress_is_recorded_and_updated() {
        let (store, _dir) = store();
        store.record_progress(summary(7), 3).unwrap();
        assert_eq!(store.history_entry(7).unwrap().unwrap().last_page, 3);

        store.record_progress(summary(7), 11).unwrap();
        assert_eq!(store.history_entry(7).unwrap().unwrap().last_page, 11);
        assert_eq!(store.history().unwrap().len(), 1, "same gallery is one entry");
    }

    #[test]
    fn history_is_listed_most_recent_first() {
        let (store, _dir) = store();
        for id in [10, 20, 30] {
            store.record_progress(summary(id), 0).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let ids: Vec<i32> = store.history().unwrap().iter().map(|h| h.summary.id).collect();
        assert_eq!(ids, vec![30, 20, 10]);
    }

    #[test]
    fn history_can_be_cleared() {
        let (store, _dir) = store();
        store.record_progress(summary(1), 0).unwrap();
        store.add_favorite(summary(1)).unwrap();

        store.clear_history().unwrap();

        assert!(store.history().unwrap().is_empty());
        assert_eq!(store.favorites().unwrap().len(), 1, "clearing history keeps favorites");
    }

    #[test]
    fn data_survives_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.redb");
        {
            let store = Store::open(&path).unwrap();
            store.add_favorite(summary(42)).unwrap();
            store.record_progress(summary(42), 9).unwrap();
        }
        let store = Store::open(&path).unwrap();
        assert!(store.favorite(42).unwrap().is_some());
        assert_eq!(store.history_entry(42).unwrap().unwrap().last_page, 9);
    }

    #[test]
    fn refuses_a_library_from_a_newer_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.redb");
        {
            let store = Store::open(&path).unwrap();
            let tx = store.db.begin_write().unwrap();
            {
                let mut meta = tx.open_table(META).unwrap();
                meta.insert("schema", "99").unwrap();
            }
            tx.commit().unwrap();
        }
        assert!(matches!(Store::open(&path), Err(StoreError::NewerSchema { .. })));
    }
}
