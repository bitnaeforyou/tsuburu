//! Dialogue search API.
//!
//! Every response says how much of the corpus is covered. A miss has to be
//! readable as "not indexed yet" rather than "does not exist", or the user
//! cannot trust the feature.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tsuburu_dialogue::{Counts, Hit, ImportSummary, Priority, Shard};

use crate::error::{ApiError, ErrorKind};
use crate::grinder::{Coverage, Grinder, GrinderSettings, GrinderStatus, ids_from_text};
use crate::state::AppState;

const MAX_RESULTS: usize = 100;

fn grinder(state: &AppState) -> Result<&Arc<Grinder>, ApiError> {
    state.grinder.as_ref().ok_or_else(|| ApiError {
        error: ErrorKind::Unsupported,
        message: "dialogue search needs text recognition, which this platform does not provide"
            .into(),
    })
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub supported: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<GrinderSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<GrinderStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counts: Option<Counts>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage: Option<Coverage>,
}

pub async fn status(State(state): State<Arc<AppState>>) -> Result<Json<StatusResponse>, ApiError> {
    let Some(grinder) = state.grinder.as_ref() else {
        return Ok(Json(StatusResponse {
            supported: false,
            settings: None,
            status: None,
            counts: None,
            coverage: None,
        }));
    };
    // Coverage needs the popularity list; if hitomi is unreachable the
    // rest of the status is still useful.
    let coverage = grinder.coverage().await.ok();
    Ok(Json(StatusResponse {
        supported: true,
        settings: Some(grinder.settings().await),
        status: Some(grinder.status().await),
        counts: Some(grinder.store().counts()?),
        coverage,
    }))
}

pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(settings): Json<GrinderSettings>,
) -> Result<Json<GrinderSettings>, ApiError> {
    let grinder = grinder(&state)?;
    if settings.kinds.is_empty() {
        return Err(ApiError::bad_request("choose at least one gallery type"));
    }
    grinder.update_settings(settings.clone()).await;
    Ok(Json(settings))
}

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    #[serde(default)]
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    25
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub hits: Vec<Hit>,
    pub counts: Counts,
}

pub async fn search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Result<Json<SearchResponse>, ApiError> {
    let grinder = grinder(&state)?;
    if params.q.trim().is_empty() {
        return Err(ApiError::bad_request("give a phrase to look for"));
    }
    let limit = params.limit.clamp(1, MAX_RESULTS);
    // The scan is CPU work over the whole store; keep it off the runtime.
    let store_grinder = Arc::clone(grinder);
    let query = params.q.clone();
    let hits = tokio::task::spawn_blocking(move || store_grinder.store().search(&query, limit))
        .await
        .map_err(|e| ApiError { error: ErrorKind::Storage, message: e.to_string() })??;
    Ok(Json(SearchResponse { hits, counts: grinder.store().counts()? }))
}

#[derive(Debug, Deserialize)]
pub struct EnqueueBody {
    /// Pasted ids or hitomi URLs, one per line or comma separated.
    pub text: String,
    #[serde(default = "default_priority")]
    pub priority: Priority,
}

fn default_priority() -> Priority {
    Priority::Imported
}

#[derive(Debug, Serialize)]
pub struct EnqueueResponse {
    pub found: usize,
    pub added: usize,
}

pub async fn enqueue(
    State(state): State<Arc<AppState>>,
    Json(body): Json<EnqueueBody>,
) -> Result<Json<EnqueueResponse>, ApiError> {
    let grinder = grinder(&state)?;
    let ids = ids_from_text(&body.text);
    if ids.is_empty() {
        return Err(ApiError::bad_request("no gallery ids found in the text"));
    }
    let added = grinder.store().enqueue(&ids, body.priority)?;
    Ok(Json(EnqueueResponse { found: ids.len(), added }))
}

#[derive(Debug, Deserialize)]
pub struct HuntBody {
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    /// How many of the matching galleries to queue, most recent first.
    #[serde(default = "default_hunt_limit")]
    pub limit: usize,
}

fn default_hunt_limit() -> usize {
    500
}

/// Narrows the corpus with the ordinary search and queues the result ahead
/// of the background sweep.
pub async fn hunt(
    State(state): State<Arc<AppState>>,
    Json(body): Json<HuntBody>,
) -> Result<Json<EnqueueResponse>, ApiError> {
    let grinder = grinder(&state)?;
    let terms = tsuburu_korean::translate(tsuburu_korean::Dictionary::embedded(), &body.q);
    let query = tsuburu_hitomi::Query {
        include: terms.iter().filter(|t| !t.excluded).map(|t| t.used.to_lowercase()).collect(),
        exclude: terms.iter().filter(|t| t.excluded).map(|t| t.used.to_lowercase()).collect(),
    };
    let filters = tsuburu_hitomi::Filters {
        language: body.language.filter(|s| !s.is_empty() && s != "all"),
        kind: body.kind.filter(|s| !s.is_empty() && s != "all"),
    };
    if query.include.is_empty() && filters.is_empty() {
        return Err(ApiError::bad_request("narrow the hunt with a term or a filter"));
    }
    let limit = body.limit.clamp(1, 5_000);
    let version = state.version().await?;
    let page = tsuburu_hitomi::search_page(
        state.fetcher.as_ref(),
        &state.cfg,
        &version,
        &tsuburu_hitomi::SearchRequest {
            query: &query,
            filters: &filters,
            sort: tsuburu_hitomi::Sort::Date,
            offset: 0,
            limit,
        },
    )
    .await?;
    let added = grinder.store().enqueue(&page.ids, Priority::Hunt)?;
    Ok(Json(EnqueueResponse { found: page.ids.len(), added }))
}

// --- exchange ---
//
// The expensive artefact is the text, not the images. Shards let one
// machine's work travel to another as a file; how the file travels is the
// user's business.

/// Galleries per shard file. Ids run past four million, so this keeps the
/// file count in the tens.
const SHARD_RANGE: i32 = 100_000;

fn shards_dir(state: &AppState) -> Result<&PathBuf, ApiError> {
    state.shards_dir.as_ref().ok_or_else(|| ApiError {
        error: ErrorKind::Storage,
        message: "no directory is configured for dialogue shards".into(),
    })
}

fn is_shard_name(name: &str) -> bool {
    // dialogue-<first>-<last>-<16 hex>.tsd ; nothing that could walk the filesystem.
    let Some(stem) = name.strip_suffix(".tsd") else { return false };
    let mut parts = stem.split('-');
    parts.next() == Some("dialogue")
        && parts.next().is_some_and(|p| p.chars().all(|c| c.is_ascii_digit()))
        && parts.next().is_some_and(|p| p.chars().all(|c| c.is_ascii_digit()))
        && parts.next().is_some_and(|p| p.len() == 16 && p.chars().all(|c| c.is_ascii_hexdigit()))
        && parts.next().is_none()
}

#[derive(Debug, Deserialize)]
pub struct ExportBody {
    /// Leave out galleries indexed on request (history, hunts): they say
    /// what the user read.
    #[serde(default)]
    pub background_only: bool,
}

#[derive(Debug, Serialize)]
pub struct ShardFile {
    pub name: String,
    pub bytes: u64,
    pub galleries: usize,
}

#[derive(Debug, Serialize)]
pub struct ExportResponse {
    pub directory: String,
    pub files: Vec<ShardFile>,
}

pub async fn export(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ExportBody>,
) -> Result<Json<ExportResponse>, ApiError> {
    let grinder = Arc::clone(grinder(&state)?);
    let dir = shards_dir(&state)?.clone();
    std::fs::create_dir_all(&dir).map_err(|e| storage_error(&e))?;

    let shards = tokio::task::spawn_blocking(move || {
        grinder.store().export_shards(SHARD_RANGE, body.background_only)
    })
    .await
    .map_err(|e| storage_error(&e))??;

    let mut files = Vec::with_capacity(shards.len());
    for shard in &shards {
        let encoded = shard.encode().map_err(|e| storage_error(&e))?;
        let name = shard.file_name(&encoded);
        std::fs::write(dir.join(&name), &encoded).map_err(|e| storage_error(&e))?;
        files.push(ShardFile { name, bytes: encoded.len() as u64, galleries: shard.entries.len() });
    }
    Ok(Json(ExportResponse { directory: dir.display().to_string(), files }))
}

pub async fn list_shards(State(state): State<Arc<AppState>>) -> Result<Json<ExportResponse>, ApiError> {
    let dir = shards_dir(&state)?.clone();
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !is_shard_name(&name) {
                continue;
            }
            let bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
            // Gallery count needs the file decoded; the name and size are enough here.
            files.push(ShardFile { name, bytes, galleries: 0 });
        }
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(ExportResponse { directory: dir.display().to_string(), files }))
}

pub async fn download_shard(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Response, ApiError> {
    if !is_shard_name(&name) {
        return Err(ApiError::bad_request("not a shard file name"));
    }
    let path = shards_dir(&state)?.join(&name);
    let bytes = tokio::fs::read(&path).await.map_err(|_| ApiError::bad_request("no such shard"))?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{name}\"")),
        ],
        bytes,
    )
        .into_response())
}

#[derive(Debug, Deserialize)]
pub struct ImportParams {
    /// The file's original name, so its hash can be checked.
    pub name: String,
}

pub async fn import(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ImportParams>,
    body: Bytes,
) -> Result<Json<ImportSummary>, ApiError> {
    let grinder = Arc::clone(grinder(&state)?);
    if !is_shard_name(&params.name) {
        return Err(ApiError::bad_request("not a shard file name"));
    }
    let shard = Shard::decode_named(&params.name, &body)
        .map_err(|e| ApiError::bad_request(format!("could not read the shard: {e}")))?;
    let summary = tokio::task::spawn_blocking(move || grinder.store().import_shard(&shard))
        .await
        .map_err(|e| storage_error(&e))??;
    Ok(Json(summary))
}

fn storage_error(err: &dyn std::fmt::Display) -> ApiError {
    ApiError { error: ErrorKind::Storage, message: err.to_string() }
}
