//! Everything a running tsuburu needs, opened and wired together.
//!
//! The desktop program and the phone one differ in how they are started and
//! what they are shown in, and in nothing else: the same stores, the same
//! sweep, the same router. This is that part, in one place, so the two cannot
//! drift apart.

use std::sync::Arc;

use tsuburu_fetch::{FetchConfig, HttpFetcher};
use tsuburu_hitomi::Config;

use crate::AppState;

#[derive(Debug)]
pub enum BootError {
    /// The databases take a lock, so there is one copy at a time. Starting a
    /// second would come up with favorites, history, dialogue and downloads
    /// all switched off, which looks like a broken program rather than a
    /// program that is already running.
    AlreadyRunning,
    Fatal(String),
}

impl std::fmt::Display for BootError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyRunning => f.write_str("tsuburu is already running"),
            Self::Fatal(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for BootError {}

/// Whether a database refused to open because another copy holds it.
///
/// redb says so in words rather than a distinct error, so this reads them.
/// Being wrong in the cautious direction only costs a clearer message.
fn already_running(message: &str) -> bool {
    let message = message.to_lowercase();
    message.contains("already open") || message.contains("acquire lock")
}

/// Opens what can be opened and reports what cannot.
///
/// Only one failure stops it: another copy already holding the databases.
/// Everything else is a feature switching itself off - the search still works
/// without a library, and saying so is more use than refusing to start.
pub async fn assemble(cfg: Config) -> Result<Arc<AppState>, BootError> {
    let store = match tsuburu_store::Store::open_default() {
        Ok(store) => {
            tracing::info!(path = %store.path().display(), "opened the library");
            Some(store)
        }
        Err(err) if already_running(&err.to_string()) => return Err(BootError::AlreadyRunning),
        Err(err) => {
            tracing::warn!(%err, "favorites and history are disabled");
            None
        }
    };

    let fetcher = Arc::new(
        HttpFetcher::new(FetchConfig::default())
            .map_err(|e| BootError::Fatal(format!("failed to build the HTTP client: {e}")))?,
    );

    let dialogue = open_dialogue();
    let grinder = match &dialogue {
        Some(dialogue) => open_grinder(&cfg, Arc::clone(dialogue))?,
        None => None,
    };
    if let Some(grinder) = &grinder {
        tokio::spawn(Arc::clone(grinder).run());
    }

    let mut state = AppState::full(fetcher, cfg, store, grinder);
    if let Some(dialogue) = dialogue {
        state = state.with_dialogue(dialogue);
    }
    if let Ok(dir) = tsuburu_store::data_dir() {
        state = state.with_shards_dir(shards_dir(&dir));
        match tsuburu_downloads::DownloadStore::open(&dir) {
            Ok(downloads) => {
                tracing::info!(path = %downloads.images_dir().display(), "opened downloads");
                state = state.with_downloads(Arc::new(downloads));
            }
            Err(err) => tracing::warn!(%err, "downloads are disabled"),
        }
        let keywords = dir.join("keywords.redb");
        if keywords.is_file() {
            match tsuburu_keywords::KeywordStore::open(&keywords) {
                Ok(keywords) => state = state.with_keywords(Arc::new(keywords)),
                Err(err) => tracing::warn!(%err, "keywords are unavailable"),
            }
        }
        let meta = dir.join("meta.redb");
        if meta.is_file() {
            match tsuburu_meta::MetaStore::open(&meta) {
                Ok(opened) => {
                    tracing::info!(path = %meta.display(), "opened the metadata snapshot");
                    state = state.with_meta(Arc::new(opened));
                }
                Err(err) => tracing::warn!(%err, "metadata snapshot is disabled"),
            }
        }
    }

    let state = Arc::new(state);
    // Switched on once, on again now: a reader who turned it on did not agree
    // to turn it on every time. Only if its files are here - nothing is
    // fetched without being asked.
    state.model.resume();
    // The sweep embeds what it reads, and the model it should embed against is
    // the one this program runs rather than an address from a setting.
    if let Some(grinder) = &state.grinder {
        grinder.use_model(Arc::clone(&state.model));
    }
    Ok(state)
}

/// Where an exported dialogue shard is written.
///
/// On a computer, beside everything else tsuburu keeps: the reader can open
/// the folder, and the file stays there until they move it.
///
/// On a phone there is no folder a reader can open - an app's own storage is
/// reachable by nothing else on the device - so the file leaves through the
/// share sheet, and the one directory the file provider may serve out of is
/// the cache. Staging it there also means a phone short of space reclaims
/// copies of files that have already been sent somewhere, which costs an
/// export rather than any data.
fn shards_dir(data: &std::path::Path) -> std::path::PathBuf {
    match tsuburu_host::cache_dir() {
        Some(cache) => cache.join("shards"),
        None => data.join("shards"),
    }
}

/// The dialogue index, which holds text and nothing else.
///
/// Opened on every platform. Searching what is in it, and taking in a corpus
/// somebody else's machine recognised, is text work - only growing it from
/// pictures needs recognition, and that is [`open_grinder`].
fn open_dialogue() -> Option<Arc<tsuburu_dialogue::DialogueStore>> {
    let opened = tsuburu_store::data_dir().map_err(|e| e.to_string()).and_then(|dir| {
        tsuburu_dialogue::DialogueStore::open(dir.join("dialogue.redb")).map_err(|e| e.to_string())
    });
    match opened {
        Ok(dialogue) => {
            tracing::info!(path = %dialogue.path().display(), "opened the dialogue index");
            Some(Arc::new(dialogue))
        }
        Err(err) => {
            tracing::warn!(%err, "dialogue search is disabled");
            None
        }
    }
}

/// The sweep that turns pages into text, which needs the platform's OCR.
///
/// Where there is none the index is still searchable; it just cannot grow
/// from pictures on this machine.
fn open_grinder(
    cfg: &Config,
    dialogue: Arc<tsuburu_dialogue::DialogueStore>,
) -> Result<Option<Arc<crate::grinder::Grinder>>, BootError> {
    let Some(ocr) = tsuburu_ocr::platform_ocr(tsuburu_ocr::OcrOptions::default()) else {
        tracing::info!("no text recognition on this platform; the index cannot grow here");
        return Ok(None);
    };
    // Its own connection pool: a page burst must not queue behind the index
    // warm-up, and must never slow down browsing.
    let fetcher = HttpFetcher::new(FetchConfig {
        max_concurrent: 8,
        // This one reads pages, not index nodes, and never looks at the same
        // one twice; it is here only so a burst does not queue behind the
        // browsing client.
        cache_bytes: 4 * 1024 * 1024,
        ..FetchConfig::default()
    })
    .map_err(|e| BootError::Fatal(format!("failed to build the indexing HTTP client: {e}")))?;
    Ok(Some(Arc::new(crate::grinder::Grinder::with_factory(
        Arc::new(fetcher),
        cfg.clone(),
        dialogue,
        Arc::from(ocr),
        tsuburu_ocr::platform_ocr,
    ))))
}
