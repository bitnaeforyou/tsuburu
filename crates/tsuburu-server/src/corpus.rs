//! Fetching the published dialogue corpus.
//!
//! Recognising a hundred thousand works takes weeks of somebody's machine.
//! The text that comes out of it is small enough to hand over, so it is
//! published as shards beside the downloads and this fetches them: one
//! button, no directory to find and no file to put anywhere.
//!
//! Shards are the same format the Settings tab exports, so importing one is
//! the same merge - what a reader has already read of their own is kept, and
//! the hash in each name is checked before any of it is believed.

use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};
use tsuburu_dialogue::Shard;
use tsuburu_fetch::HttpFetcher;

use crate::grinder::Grinder;

/// Which release carries the corpus. A tag of its own, so publishing a new
/// program does not mean publishing four hundred megabytes with it.
const TAG: &str = "corpus";

const RELEASES: Option<&str> = option_env!("TSUBURU_RELEASES");

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Progress {
    Idle,
    Fetching { done: usize, total: usize, works: usize },
    Ready { works: usize },
    Failed { error: String },
}

#[derive(Deserialize)]
struct Release {
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
}

#[derive(Debug)]
pub struct Corpus {
    progress: RwLock<Progress>,
}

impl Default for Corpus {
    fn default() -> Self {
        Self { progress: RwLock::new(Progress::Idle) }
    }
}

impl Corpus {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Whether there is anywhere to fetch from. A build that was not
    /// published does not know where it came from.
    pub fn publishable(&self) -> bool {
        RELEASES.is_some()
    }

    pub fn progress(&self) -> Progress {
        self.progress.read().map(|p| p.clone()).unwrap_or(Progress::Idle)
    }

    fn set(&self, next: Progress) {
        if let Ok(mut held) = self.progress.write() {
            *held = next;
        }
    }

    /// Downloads every shard of the corpus and merges it in.
    ///
    /// Each shard is imported as it arrives rather than after all of them
    /// have: four hundred megabytes held in memory to be written at the end
    /// is four hundred megabytes that a closed lid loses.
    pub fn fetch(self: &Arc<Self>, grinder: Arc<Grinder>, fetcher: Arc<HttpFetcher>) {
        if matches!(self.progress(), Progress::Fetching { .. }) {
            return;
        }
        self.set(Progress::Fetching { done: 0, total: 0, works: 0 });
        let me = Arc::clone(self);
        tokio::spawn(async move {
            match me.run(grinder, fetcher).await {
                Ok(works) => me.set(Progress::Ready { works }),
                Err(error) => {
                    tracing::warn!(%error, "the corpus could not be fetched");
                    me.set(Progress::Failed { error })
                }
            }
        });
    }

    async fn run(&self, grinder: Arc<Grinder>, fetcher: Arc<HttpFetcher>) -> Result<usize, String> {
        let repo = RELEASES.ok_or("this build does not say where it was published from")?;
        let url = format!("https://api.github.com/repos/{repo}/releases/tags/{TAG}");
        let body = fetcher
            .stream(&url)
            .await
            .map_err(|e| format!("could not reach the corpus: {e}"))?
            .text()
            .await
            .map_err(|e| format!("could not read the corpus listing: {e}"))?;
        let release: Release = serde_json::from_str(&body)
            .map_err(|e| format!("the corpus listing is not what was expected: {e}"))?;

        let shards: Vec<Asset> =
            release.assets.into_iter().filter(|a| a.name.ends_with(".tsd")).collect();
        if shards.is_empty() {
            return Err("the corpus release carries no shards".into());
        }
        let total = shards.len();
        tracing::info!(
            total,
            bytes = shards.iter().map(|a| a.size).sum::<u64>(),
            "fetching the corpus"
        );

        let mut works = 0usize;
        for (done, asset) in shards.into_iter().enumerate() {
            self.set(Progress::Fetching { done, total, works });
            let bytes = fetcher
                .stream(&asset.browser_download_url)
                .await
                .map_err(|e| format!("{}: {e}", asset.name))?
                .bytes()
                .await
                .map_err(|e| format!("{}: {e}", asset.name))?;
            // The hash is in the name; a shard that does not match it is not
            // the shard that was published.
            let shard = Shard::decode_named(&asset.name, &bytes)
                .map_err(|e| format!("{}: {e}", asset.name))?;
            let store = grinder.store_handle();
            let summary = tokio::task::spawn_blocking(move || store.import_shard(&shard))
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| format!("{}: {e}", asset.name))?;
            works += summary.added;
            self.set(Progress::Fetching { done: done + 1, total, works });
        }
        tracing::info!(works, "the corpus is in");
        Ok(works)
    }
}
