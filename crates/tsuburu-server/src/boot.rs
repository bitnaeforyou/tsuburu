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

    let grinder = open_grinder(&cfg)?;
    if let Some(grinder) = &grinder {
        tokio::spawn(Arc::clone(grinder).run());
    }

    let mut state = AppState::full(fetcher, cfg, store, grinder);
    if let Ok(dir) = tsuburu_store::data_dir() {
        state = state.with_shards_dir(dir.join("shards"));
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

/// Dialogue indexing needs the platform's OCR. Where there is none the feature
/// is reported as unsupported rather than silently missing.
fn open_grinder(cfg: &Config) -> Result<Option<Arc<crate::grinder::Grinder>>, BootError> {
    let Some(ocr) = tsuburu_ocr::platform_ocr(tsuburu_ocr::OcrOptions::default()) else {
        tracing::info!("no text recognition on this platform; dialogue search disabled");
        return Ok(None);
    };
    let opened = tsuburu_store::data_dir().map_err(|e| e.to_string()).and_then(|dir| {
        tsuburu_dialogue::DialogueStore::open(dir.join("dialogue.redb")).map_err(|e| e.to_string())
    });
    let dialogue = match opened {
        Ok(dialogue) => dialogue,
        Err(err) => {
            tracing::warn!(%err, "dialogue search is disabled");
            return Ok(None);
        }
    };
    tracing::info!(path = %dialogue.path().display(), "opened the dialogue index");
    // Its own connection pool: a page burst must not queue behind the index
    // warm-up, and must never slow down browsing.
    let fetcher = HttpFetcher::new(FetchConfig {
        max_concurrent: 8,
        cache_entries: 256,
        ..FetchConfig::default()
    })
    .map_err(|e| BootError::Fatal(format!("failed to build the indexing HTTP client: {e}")))?;
    Ok(Some(Arc::new(crate::grinder::Grinder::with_factory(
        Arc::new(fetcher),
        cfg.clone(),
        Arc::new(dialogue),
        Arc::from(ocr),
        tsuburu_ocr::platform_ocr,
    ))))
}
