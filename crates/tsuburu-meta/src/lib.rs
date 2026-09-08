//! A local snapshot of gallery metadata.
//!
//! The first design deliberately avoided downloading a metadata snapshot
//! (hundreds of megabytes, periodic syncing). artifact's `data.db` is exactly
//! that snapshot, and when the user already has it, keeping it locally turns
//! cards into a lookup instead of a 200 KB fetch and makes Korean titles,
//! artists, series and characters searchable offline.
//!
//! Anything newer than the snapshot still goes to the network; callers
//! check [`MetaStore::latest_id`].

use redb::{
    Database, MultimapTableDefinition, ReadableTable, ReadableTableMetadata, TableDefinition,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tsuburu_text::{codes, codes_into};

const WORKS: TableDefinition<i32, &[u8]> = TableDefinition::new("works");
/// id -> the title's match codes. Scanning these avoids decoding a work and
/// re-normalising 1.4 million titles on every query.
const TITLES: TableDefinition<i32, &[u8]> = TableDefinition::new("titles");
/// "tag:glasses" -> ids. Sorted keys make prefix suggestions a range scan.
const TERMS: MultimapTableDefinition<&str, i32> = MultimapTableDefinition::new("terms");
const META: TableDefinition<&str, &str> = TableDefinition::new("meta");
/// 2: titles stored as match codes.
const SCHEMA_VERSION: &str = "2";

#[derive(Debug, thiserror::Error)]
pub enum MetaError {
    #[error("could not open the metadata database: {0}")]
    Open(String),
    #[error("metadata database error: {0}")]
    Database(String),
    #[error("stored record could not be read: {0}")]
    Corrupt(String),
    #[error("this metadata snapshot was written by a newer tsuburu (schema {found})")]
    NewerSchema { found: String },
    #[error("this metadata snapshot uses an old layout (schema {found}); import it again")]
    OlderSchema { found: String },
}

impl From<serde_json::Error> for MetaError {
    fn from(err: serde_json::Error) -> Self {
        MetaError::Corrupt(err.to_string())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Work {
    pub id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub language: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artists: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub series: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub characters: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Unix seconds, when the source had a usable date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published: Option<i64>,
    #[serde(default)]
    pub pages: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_hash: Option<String>,
    #[serde(default)]
    pub exists: bool,
}

impl Work {
    /// Every key this work is findable under.
    fn term_keys(&self) -> Vec<String> {
        let mut keys = Vec::new();
        let norm = |s: &str| s.trim().to_lowercase();
        if !self.kind.is_empty() {
            keys.push(format!("type:{}", norm(&self.kind)));
        }
        if !self.language.is_empty() {
            keys.push(format!("lang:{}", norm(&self.language)));
        }
        for (ns, values) in [
            ("artist", &self.artists),
            ("group", &self.groups),
            ("series", &self.series),
            ("character", &self.characters),
            ("tag", &self.tags),
        ] {
            for v in values {
                let v = norm(v);
                if !v.is_empty() {
                    keys.push(format!("{ns}:{v}"));
                }
            }
        }
        keys
    }
}

/// What to look for. Terms are namespaced (`tag:glasses`, `artist:keso`);
/// a bare word is tried in every namespace.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetaQuery {
    pub title: Option<String>,
    pub terms: Vec<String>,
    pub language: Option<String>,
    pub kind: Option<String>,
    pub exists_only: bool,
}

impl MetaQuery {
    pub fn is_empty(&self) -> bool {
        self.title.as_deref().is_none_or(|t| t.trim().is_empty())
            && self.terms.is_empty()
            && self.language.is_none()
            && self.kind.is_none()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Page {
    pub total: usize,
    /// Newest first.
    pub ids: Vec<i32>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ImportSummary {
    pub works: usize,
    pub added: usize,
}

const NAMESPACES: [&str; 5] = ["tag", "artist", "series", "character", "group"];

pub struct MetaStore {
    db: Database,
    path: PathBuf,
}

impl MetaStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, MetaError> {
        let path = path.as_ref().to_path_buf();
        let db = Database::create(&path).map_err(|e| MetaError::Open(e.to_string()))?;
        let store = Self { db, path };
        store.init_schema()?;
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn init_schema(&self) -> Result<(), MetaError> {
        let tx = self.db.begin_write().map_err(db_err)?;
        {
            tx.open_table(WORKS).map_err(db_err)?;
            tx.open_table(TITLES).map_err(db_err)?;
            tx.open_multimap_table(TERMS).map_err(db_err)?;
            let mut meta = tx.open_table(META).map_err(db_err)?;
            let existing = meta.get("schema").map_err(db_err)?.map(|v| v.value().to_string());
            match existing {
                None => {
                    meta.insert("schema", SCHEMA_VERSION).map_err(db_err)?;
                }
                Some(found) if found == SCHEMA_VERSION => {}
                Some(found) if found.as_str() < SCHEMA_VERSION => {
                    return Err(MetaError::OlderSchema { found });
                }
                Some(found) => return Err(MetaError::NewerSchema { found }),
            }
        }
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    /// Highest id in the snapshot; anything above it is not covered.
    pub fn latest_id(&self) -> Result<Option<i32>, MetaError> {
        let tx = self.db.begin_read().map_err(db_err)?;
        let works = tx.open_table(WORKS).map_err(db_err)?;
        Ok(works.last().map_err(db_err)?.map(|(k, _)| k.value()))
    }

    pub fn count(&self) -> Result<usize, MetaError> {
        let tx = self.db.begin_read().map_err(db_err)?;
        let works = tx.open_table(WORKS).map_err(db_err)?;
        Ok(works.len().map_err(db_err)? as usize)
    }

    /// Stores works in batches; an id already present is replaced.
    pub fn import_works<I>(
        &self,
        works: I,
        batch: usize,
        mut progress: impl FnMut(usize),
    ) -> Result<ImportSummary, MetaError>
    where
        I: IntoIterator<Item = Work>,
    {
        let batch = batch.max(1);
        let mut summary = ImportSummary::default();
        let mut pending: Vec<Work> = Vec::with_capacity(batch);

        let flush = |pending: &mut Vec<Work>| -> Result<(), MetaError> {
            if pending.is_empty() {
                return Ok(());
            }
            let tx = self.db.begin_write().map_err(db_err)?;
            {
                let mut table = tx.open_table(WORKS).map_err(db_err)?;
                let mut titles = tx.open_table(TITLES).map_err(db_err)?;
                let mut terms = tx.open_multimap_table(TERMS).map_err(db_err)?;
                for work in pending.drain(..) {
                    let encoded = lz4_flex::compress_prepend_size(&serde_json::to_vec(&work)?);
                    table.insert(work.id, encoded.as_slice()).map_err(db_err)?;
                    titles.insert(work.id, codes(&work.title).as_slice()).map_err(db_err)?;
                    for key in work.term_keys() {
                        terms.insert(key.as_str(), work.id).map_err(db_err)?;
                    }
                }
            }
            tx.commit().map_err(db_err)?;
            Ok(())
        };

        for work in works {
            summary.works += 1;
            summary.added += 1;
            pending.push(work);
            if pending.len() >= batch {
                flush(&mut pending)?;
                progress(summary.works);
            }
        }
        flush(&mut pending)?;
        progress(summary.works);
        Ok(summary)
    }

    pub fn work(&self, id: i32) -> Result<Option<Work>, MetaError> {
        let tx = self.db.begin_read().map_err(db_err)?;
        let works = tx.open_table(WORKS).map_err(db_err)?;
        match works.get(id).map_err(db_err)? {
            Some(v) => Ok(Some(decode_work(v.value())?)),
            None => Ok(None),
        }
    }

    pub fn works(&self, ids: &[i32]) -> Result<Vec<Work>, MetaError> {
        let tx = self.db.begin_read().map_err(db_err)?;
        let works = tx.open_table(WORKS).map_err(db_err)?;
        let mut out = Vec::with_capacity(ids.len());
        for &id in ids {
            if let Some(v) = works.get(id).map_err(db_err)? {
                out.push(decode_work(v.value())?);
            }
        }
        Ok(out)
    }

    /// Ids under one exact key, e.g. `tag:glasses`.
    fn ids_for(&self, tx: &redb::ReadTransaction, key: &str) -> Result<HashSet<i32>, MetaError> {
        let terms = tx.open_multimap_table(TERMS).map_err(db_err)?;
        let mut out = HashSet::new();
        for v in terms.get(key).map_err(db_err)? {
            out.insert(v.map_err(db_err)?.value());
        }
        Ok(out)
    }

    /// A bare term is looked up in every namespace and the results merged.
    /// Tags carry a `female:`/`male:` prefix in the source, so a bare
    /// `glasses` also tries both, the way hitomi's own search reads it.
    fn ids_for_term(
        &self,
        tx: &redb::ReadTransaction,
        term: &str,
    ) -> Result<HashSet<i32>, MetaError> {
        let term = term.trim().to_lowercase();
        if let Some(tag) = term.strip_prefix("tag:") {
            let mut out = self.ids_for(tx, &term)?;
            if !tag.starts_with("female:") && !tag.starts_with("male:") {
                out.extend(self.ids_for(tx, &format!("tag:female:{tag}"))?);
                out.extend(self.ids_for(tx, &format!("tag:male:{tag}"))?);
            }
            return Ok(out);
        }
        if NAMESPACES.iter().any(|ns| term.starts_with(&format!("{ns}:"))) {
            return self.ids_for(tx, &term);
        }
        let mut out = HashSet::new();
        for ns in NAMESPACES {
            out.extend(self.ids_for(tx, &format!("{ns}:{term}"))?);
        }
        if !term.starts_with("female:") && !term.starts_with("male:") {
            out.extend(self.ids_for(tx, &format!("tag:female:{term}"))?);
            out.extend(self.ids_for(tx, &format!("tag:male:{term}"))?);
        }
        Ok(out)
    }

    /// Evaluates a query. Terms and filters intersect; the title, when
    /// given, is matched as a jamo-normalised substring so Korean titles
    /// match regardless of spacing.
    pub fn search(
        &self,
        query: &MetaQuery,
        offset: usize,
        limit: usize,
    ) -> Result<Page, MetaError> {
        if query.is_empty() {
            return Ok(Page::default());
        }
        let tx = self.db.begin_read().map_err(db_err)?;

        let mut candidates: Option<HashSet<i32>> = None;
        let mut narrow = |set: HashSet<i32>| {
            candidates = Some(match candidates.take() {
                None => set,
                Some(prev) => prev.intersection(&set).copied().collect(),
            });
        };
        for term in &query.terms {
            narrow(self.ids_for_term(&tx, term)?);
        }
        if let Some(language) = &query.language {
            narrow(self.ids_for(&tx, &format!("lang:{}", language.to_lowercase()))?);
        }
        if let Some(kind) = &query.kind {
            narrow(self.ids_for(&tx, &format!("type:{}", kind.to_lowercase()))?);
        }

        let mut ids: Vec<i32> = match (&query.title, candidates) {
            (Some(title), candidates) if !title.trim().is_empty() => {
                let mut needle = Vec::new();
                codes_into(title, &mut needle);
                // The scan opens its own read transactions; redb allows
                // several at once, so this one can stay open for the filter.
                self.scan_titles(&needle, candidates.as_ref())?
            }
            (_, Some(candidates)) => candidates.into_iter().collect(),
            (_, None) => Vec::new(),
        };

        if query.exists_only {
            let works = tx.open_table(WORKS).map_err(db_err)?;
            ids.retain(|id| {
                works
                    .get(*id)
                    .ok()
                    .flatten()
                    .and_then(|v| decode_work(v.value()).ok())
                    .is_some_and(|w| w.exists)
            });
        }

        ids.sort_unstable_by(|a, b| b.cmp(a));
        let total = ids.len();
        Ok(Page { total, ids: ids.into_iter().skip(offset).take(limit).collect() })
    }

    /// Every title is a candidate, so the scan is split across cores. Ranges
    /// are cut by count rather than id span because galleries cluster in
    /// recent ids.
    fn scan_titles(
        &self,
        needle: &[u8],
        candidates: Option<&HashSet<i32>>,
    ) -> Result<Vec<i32>, MetaError> {
        let keys: Vec<i32> = {
            let tx = self.db.begin_read().map_err(db_err)?;
            let titles = tx.open_table(TITLES).map_err(db_err)?;
            let mut keys = Vec::new();
            for row in titles.iter().map_err(db_err)? {
                keys.push(row.map_err(db_err)?.0.value());
            }
            keys
        };
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let threads =
            std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 16);
        let per = keys.len().div_ceil(threads);
        let ranges: Vec<(i32, i32)> = keys.chunks(per).map(|c| (c[0], c[c.len() - 1])).collect();

        Ok(std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(ranges.len());
            for (lo, hi) in ranges {
                handles.push(scope.spawn(move || -> Result<Vec<i32>, MetaError> {
                    let tx = self.db.begin_read().map_err(db_err)?;
                    let titles = tx.open_table(TITLES).map_err(db_err)?;
                    let mut out = Vec::new();
                    for row in titles.range(lo..=hi).map_err(db_err)? {
                        let (key, value) = row.map_err(db_err)?;
                        let id = key.value();
                        if candidates.is_some_and(|c| !c.contains(&id)) {
                            continue;
                        }
                        if memchr::memmem::find(value.value(), needle).is_some() {
                            out.push(id);
                        }
                    }
                    Ok(out)
                }));
            }
            let mut all = Vec::new();
            for handle in handles {
                match handle.join() {
                    Ok(Ok(part)) => all.extend(part),
                    Ok(Err(err)) => tracing::warn!(%err, "a title scan thread failed"),
                    Err(_) => tracing::warn!("a title scan thread panicked"),
                }
            }
            all
        }))
    }

    /// Keys starting with `prefix`, for autocompletion: `artist:ke` -> `artist:keso`.
    pub fn suggest(&self, prefix: &str, limit: usize) -> Result<Vec<(String, usize)>, MetaError> {
        let prefix = prefix.trim().to_lowercase();
        if prefix.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let tx = self.db.begin_read().map_err(db_err)?;
        let terms = tx.open_multimap_table(TERMS).map_err(db_err)?;
        let mut out = Vec::new();
        for row in terms.range(prefix.as_str()..).map_err(db_err)? {
            let (key, values) = row.map_err(db_err)?;
            let key = key.value().to_string();
            if !key.starts_with(&prefix) {
                break;
            }
            out.push((key, values.count()));
            if out.len() >= limit {
                break;
            }
        }
        Ok(out)
    }
}

fn decode_work(bytes: &[u8]) -> Result<Work, MetaError> {
    let raw = lz4_flex::decompress_size_prepended(bytes)
        .map_err(|e| MetaError::Corrupt(e.to_string()))?;
    Ok(serde_json::from_slice(&raw)?)
}

fn db_err(err: impl std::fmt::Display) -> MetaError {
    MetaError::Database(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (MetaStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        (MetaStore::open(dir.path().join("meta.redb")).unwrap(), dir)
    }

    fn work(id: i32, title: &str, artist: &str, tags: &[&str], lang: &str, kind: &str) -> Work {
        Work {
            id,
            title: title.into(),
            kind: kind.into(),
            language: lang.into(),
            artists: vec![artist.into()],
            tags: tags.iter().map(|t| t.to_string()).collect(),
            pages: 20,
            exists: true,
            ..Work::default()
        }
    }

    fn seeded() -> (MetaStore, tempfile::TempDir) {
        let (store, dir) = store();
        store
            .import_works(
                vec![
                    work(
                        1,
                        "Drip Coffee | 드립 커피→프롬♡유",
                        "borusiti",
                        &["female:big breasts"],
                        "korean",
                        "manga",
                    ),
                    work(2, "Emma Chuui", "keso", &["female:glasses"], "japanese", "doujinshi"),
                    work(
                        3,
                        "Maison Inkaku | 메종음핵",
                        "keso",
                        &["female:glasses", "female:big breasts"],
                        "korean",
                        "manga",
                    ),
                ],
                2,
                |_| {},
            )
            .unwrap();
        (store, dir)
    }

    #[test]
    fn works_round_trip_and_latest_id() {
        let (store, _d) = seeded();
        assert_eq!(store.count().unwrap(), 3);
        assert_eq!(store.latest_id().unwrap(), Some(3));
        let w = store.work(3).unwrap().unwrap();
        assert_eq!(w.artists, vec!["keso"]);
        assert_eq!(store.works(&[1, 99, 2]).unwrap().len(), 2);
    }

    #[test]
    fn terms_intersect_and_filters_apply() {
        let (store, _d) = seeded();
        let q = MetaQuery { terms: vec!["artist:keso".into()], ..MetaQuery::default() };
        assert_eq!(store.search(&q, 0, 10).unwrap().ids, vec![3, 2]);

        let q = MetaQuery {
            terms: vec!["keso".into(), "female:big breasts".into()],
            ..MetaQuery::default()
        };
        assert_eq!(store.search(&q, 0, 10).unwrap().ids, vec![3]);

        let q = MetaQuery {
            terms: vec!["glasses".into()],
            language: Some("korean".into()),
            ..MetaQuery::default()
        };
        assert_eq!(store.search(&q, 0, 10).unwrap().ids, vec![3]);
    }

    #[test]
    fn explicit_tag_terms_also_cover_gendered_variants() {
        let (store, _d) = seeded();
        let q = MetaQuery {
            terms: vec!["tag:glasses".into(), "artist:keso".into()],
            ..MetaQuery::default()
        };
        assert_eq!(store.search(&q, 0, 10).unwrap().ids, vec![3, 2]);
    }

    #[test]
    fn bare_terms_search_every_namespace() {
        let (store, _d) = seeded();
        let q = MetaQuery { terms: vec!["borusiti".into()], ..MetaQuery::default() };
        assert_eq!(store.search(&q, 0, 10).unwrap().ids, vec![1]);
    }

    #[test]
    fn korean_titles_match_regardless_of_spacing() {
        let (store, _d) = seeded();
        let q = MetaQuery { title: Some("메종 음핵".into()), ..MetaQuery::default() };
        assert_eq!(store.search(&q, 0, 10).unwrap().ids, vec![3]);
        let q = MetaQuery {
            title: Some("드립커피".into()),
            kind: Some("manga".into()),
            ..MetaQuery::default()
        };
        assert_eq!(store.search(&q, 0, 10).unwrap().ids, vec![1]);
    }

    #[test]
    fn paging_and_totals() {
        let (store, _d) = seeded();
        let q = MetaQuery { kind: Some("manga".into()), ..MetaQuery::default() };
        let page = store.search(&q, 1, 1).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.ids, vec![1]);
    }

    #[test]
    fn suggestions_are_prefix_scans() {
        let (store, _d) = seeded();
        let s = store.suggest("artist:ke", 5).unwrap();
        assert_eq!(s, vec![("artist:keso".to_string(), 2)]);
        assert!(
            store
                .suggest("tag:female:g", 5)
                .unwrap()
                .iter()
                .any(|(k, _)| k == "tag:female:glasses")
        );
    }

    #[test]
    fn empty_query_returns_nothing() {
        let (store, _d) = seeded();
        assert_eq!(store.search(&MetaQuery::default(), 0, 10).unwrap(), Page::default());
    }
}
