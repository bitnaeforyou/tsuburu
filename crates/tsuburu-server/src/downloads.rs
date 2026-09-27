//! Keeping pages on disk and reading them without the network.
//!
//! A download is a background job: the pages are fetched under the same
//! politeness cap as everything else, written by hash, and recorded so the
//! reader can open the work later with hitomi unreachable.

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tsuburu_downloads::{Download, DownloadStore, DownloadedPage, Progress, now_millis};
use tsuburu_hitomi::Fetcher;

use crate::error::{ApiError, ErrorKind};
use crate::state::AppState;

/// Pages in flight for one job. The CDN gives about 150 KB/s per connection,
/// so a handful at once is the difference between minutes and an hour.
const PAGE_WORKERS: usize = 6;

fn store(state: &AppState) -> Result<&Arc<DownloadStore>, ApiError> {
    state.downloads.as_ref().ok_or_else(|| ApiError {
        error: ErrorKind::Storage,
        message: "downloads are unavailable: the store could not be opened".into(),
        code: None,
    })
}

/// What a job is doing, for the progress display.
#[derive(Debug, Clone, Default, Serialize)]
pub struct JobStatus {
    pub running: bool,
    pub wanted: usize,
    pub fetched: usize,
    pub failed: usize,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Jobs {
    inner: Mutex<HashMap<i32, JobStatus>>,
}

impl Jobs {
    pub fn status(&self, id: i32) -> Option<JobStatus> {
        self.inner.lock().ok()?.get(&id).cloned()
    }

    fn set(&self, id: i32, status: JobStatus) {
        if let Ok(mut jobs) = self.inner.lock() {
            jobs.insert(id, status);
        }
    }

    fn update(&self, id: i32, f: impl FnOnce(&mut JobStatus)) {
        if let Ok(mut jobs) = self.inner.lock()
            && let Some(status) = jobs.get_mut(&id)
        {
            f(status);
        }
    }

    /// Marks a work as being fetched, unless it already is.
    ///
    /// Checking and then marking were two steps with the network in between,
    /// so two presses of Download both passed the check and the second reset
    /// the first's count to zero.
    fn claim(&self, id: i32) -> bool {
        let Ok(mut jobs) = self.inner.lock() else { return false };
        if jobs.get(&id).is_some_and(|status| status.running) {
            return false;
        }
        jobs.insert(id, JobStatus { running: true, ..JobStatus::default() });
        true
    }

    /// Gives it back, for a start that got no further.
    fn release(&self, id: i32) {
        if let Ok(mut jobs) = self.inner.lock() {
            jobs.remove(&id);
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct StartBody {
    /// Page numbers to fetch, 0-based. Empty means the whole work.
    #[serde(default)]
    pub pages: Vec<u16>,
}

#[derive(Debug, Serialize)]
pub struct StartResponse {
    pub id: i32,
    pub wanted: usize,
}

/// Downloads a whole work, or the pages named.
pub async fn start(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
    Json(body): Json<StartBody>,
) -> Result<Json<StartResponse>, ApiError> {
    // Claimed before anything is fetched, because fetching is where the two
    // presses of Download used to pass each other.
    if !state.download_jobs.claim(id) {
        return Err(ApiError::bad_request("that gallery is already downloading"));
    }
    let started = begin(&state, id, body).await;
    if started.is_err() {
        state.download_jobs.release(id);
    }
    started
}

async fn begin(
    state: &Arc<AppState>,
    id: i32,
    body: StartBody,
) -> Result<Json<StartResponse>, ApiError> {
    let downloads = Arc::clone(store(state)?);

    let gallery = tsuburu_hitomi::fetch_gallery(state.fetcher.as_ref(), &state.cfg, id).await?;
    let gg = tsuburu_hitomi::fetch_gg(state.fetcher.as_ref(), &state.cfg).await?;

    let all: Vec<DownloadedPage> = gallery
        .files
        .iter()
        .enumerate()
        .map(|(i, file)| DownloadedPage {
            page: i as u16,
            hash: file.hash.clone(),
            ext: tsuburu_hitomi::image::image_extension(file).to_string(),
            width: file.width,
            height: file.height,
        })
        .collect();

    // The record always lists the whole work, so a partial download still
    // knows how long the thing is and which pages are missing.
    let record = Download {
        id,
        title: gallery.title.clone(),
        language: gallery.language.clone(),
        kind: gallery.kind.clone(),
        pages: all.clone(),
        added_at: downloads.get(id)?.map(|d| d.added_at).unwrap_or_else(now_millis),
    };
    downloads.put(&record)?;

    let wanted: Vec<DownloadedPage> = all
        .iter()
        .filter(|p| body.pages.is_empty() || body.pages.contains(&p.page))
        .cloned()
        .collect();
    if wanted.is_empty() {
        return Err(ApiError::bad_request("no such page in that gallery"));
    }
    let missing: Vec<_> =
        wanted.into_iter().filter(|p| !downloads.has_image(&p.hash, &p.ext)).collect();

    let response = StartResponse { id, wanted: missing.len() };
    if missing.is_empty() {
        state.download_jobs.set(id, JobStatus::default());
        return Ok(Json(response));
    }

    state
        .download_jobs
        .set(id, JobStatus { running: true, wanted: missing.len(), ..JobStatus::default() });

    let fetcher = Arc::clone(&state.fetcher);
    let cfg = state.cfg.clone();
    let jobs = Arc::clone(&state.download_jobs);
    tokio::spawn(async move {
        let limiter = Arc::new(tokio::sync::Semaphore::new(PAGE_WORKERS));
        let mut tasks = tokio::task::JoinSet::new();
        for page in missing {
            let Ok(url) = tsuburu_hitomi::image_url(
                &cfg,
                &gg,
                &tsuburu_hitomi::GalleryFile {
                    hash: page.hash.clone(),
                    name: String::new(),
                    width: 0,
                    height: 0,
                    hasavif: u8::from(page.ext == "avif"),
                },
            ) else {
                jobs.update(id, |s| s.failed += 1);
                continue;
            };
            let permit = match Arc::clone(&limiter).acquire_owned().await {
                Ok(permit) => permit,
                Err(_) => break,
            };
            let fetcher = Arc::clone(&fetcher);
            let downloads = Arc::clone(&downloads);
            let jobs = Arc::clone(&jobs);
            tasks.spawn(async move {
                let _permit = permit;
                match fetcher.get(&url).await {
                    Ok(bytes) => match downloads.save_image(&page.hash, &page.ext, &bytes) {
                        Ok(()) => jobs.update(id, |s| s.fetched += 1),
                        Err(err) => {
                            tracing::warn!(id, page = page.page, %err, "could not store a page");
                            jobs.update(id, |s| s.failed += 1);
                        }
                    },
                    Err(err) => {
                        tracing::debug!(id, page = page.page, %err, "page download failed");
                        jobs.update(id, |s| s.failed += 1);
                    }
                }
            });
        }
        while tasks.join_next().await.is_some() {}
        jobs.update(id, |s| s.running = false);
    });

    Ok(Json(response))
}

#[derive(Debug, Serialize)]
pub struct DownloadStatus {
    #[serde(flatten)]
    pub progress: Progress,
    pub complete: bool,
    pub job: JobStatus,
    /// The shelf the work is on, which is the library's to say: a shelf
    /// belongs to the work, not to the copy of it that happens to be on
    /// disk. Absent where there is no library to ask.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
}

pub async fn status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
) -> Result<Json<DownloadStatus>, ApiError> {
    let progress = store(&state)?
        .progress(id)?
        .ok_or_else(|| ApiError::bad_request("that gallery has not been downloaded"))?;
    Ok(Json(DownloadStatus {
        complete: progress.complete(),
        job: state.download_jobs.status(id).unwrap_or_default(),
        folder: state.store.as_ref().and_then(|s| s.folder(id).ok()).flatten(),
        progress,
    }))
}

#[derive(Debug, Serialize)]
pub struct ListResponse {
    pub items: Vec<DownloadStatus>,
    pub bytes: u64,
}

pub async fn list(State(state): State<Arc<AppState>>) -> Result<Json<ListResponse>, ApiError> {
    let shelves = state.store.as_ref().and_then(|s| s.folder_map().ok()).unwrap_or_default();
    let items: Vec<DownloadStatus> = store(&state)?
        .list()?
        .into_iter()
        .map(|progress| DownloadStatus {
            complete: progress.complete(),
            job: state.download_jobs.status(progress.id).unwrap_or_default(),
            folder: shelves.get(&progress.id).cloned(),
            progress,
        })
        .collect();
    let bytes = items.iter().map(|i| i.progress.bytes).sum();
    Ok(Json(ListResponse { items, bytes }))
}

#[derive(Debug, Serialize)]
pub struct Removed {
    pub removed: bool,
}

pub async fn remove(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
) -> Result<Json<Removed>, ApiError> {
    Ok(Json(Removed { removed: store(&state)?.remove(id)? }))
}

/// The stored page list for a downloaded work.
///
/// The gallery endpoint falls back to this when hitomi cannot be reached,
/// which is what makes a downloaded work readable offline.
pub fn stored_gallery(state: &AppState, id: i32) -> Option<Download> {
    state.downloads.as_ref()?.get(id).ok().flatten()
}
