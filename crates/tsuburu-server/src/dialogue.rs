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
use crate::grinder::{
    Coverage, Grinder, GrinderSettings, GrinderStatus, ImportProgress, ids_from_text,
};
use crate::state::AppState;

const MAX_RESULTS: usize = 100;

fn grinder(state: &AppState) -> Result<&Arc<Grinder>, ApiError> {
    state.grinder.as_ref().ok_or_else(|| ApiError {
        error: ErrorKind::Unsupported,
        message: "dialogue search needs text recognition, which this platform does not provide"
            .into(),
        code: Some("no_recognition"),
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub import: Option<ImportProgress>,
    /// What the platform is missing, when installing it would help.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

pub async fn status(State(state): State<Arc<AppState>>) -> Result<Json<StatusResponse>, ApiError> {
    let Some(grinder) = state.grinder.as_ref() else {
        return Ok(Json(StatusResponse {
            supported: false,
            settings: None,
            status: None,
            counts: None,
            coverage: None,
            import: None,
            note: tsuburu_ocr::platform_note(),
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
        import: grinder.import_progress(),
        note: None,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ImportArtifactBody {
    /// Path to the `llm-search-index` directory on this machine.
    pub dir: String,
}

/// Loads a recognised Korean corpus. Runs in the background;
/// `status` reports progress.
pub async fn import_artifact(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ImportArtifactBody>,
) -> Result<Json<ImportProgress>, ApiError> {
    let grinder = grinder(&state)?;
    let dir = PathBuf::from(body.dir.trim());
    if !dir.is_dir() {
        return Err(ApiError::bad_request(format!("{} is not a directory", dir.display())));
    }
    grinder.start_artifact_import(dir).map_err(ApiError::bad_request)?;
    Ok(Json(grinder.import_progress().unwrap_or_default()))
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
        .map_err(|e| ApiError {
            error: ErrorKind::Storage,
            message: e.to_string(),
            code: None,
        })??;
    Ok(Json(SearchResponse { hits, counts: grinder.store().counts()? }))
}

#[derive(Debug, Deserialize)]
pub struct EnqueueBody {
    /// Pasted ids or hitomi URLs, one per line or comma separated.
    pub text: String,
    #[serde(default = "default_priority")]
    pub priority: Priority,
    /// Re-recognise galleries that already have text.
    #[serde(default)]
    pub force: bool,
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
    let added = grinder.store().enqueue_with(&ids, body.priority, body.force)?;
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
    /// Re-recognise galleries that already have text.
    #[serde(default)]
    pub force: bool,
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
    let added = grinder.store().enqueue_with(&page.ids, Priority::Hunt, body.force)?;
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
        code: None,
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

pub async fn list_shards(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ExportResponse>, ApiError> {
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
    ApiError { error: ErrorKind::Storage, message: err.to_string(), code: None }
}

// --- similar scenes ---

#[derive(Debug, Deserialize)]
pub struct SimilarParams {
    pub id: i32,
    #[serde(default)]
    pub page: u16,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Debug, Serialize)]
pub struct SimilarHit {
    pub gallery_id: i32,
    pub page: u16,
    pub score: f32,
    pub snippet: Vec<String>,
}

/// Passages closest in meaning to the one being read.
///
/// The neighbours come from the imported embeddings; the text shown beside them
/// comes from the local corpus, so nothing is fetched.
pub async fn similar(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SimilarParams>,
) -> Result<Json<Vec<SimilarHit>>, ApiError> {
    let grinder = Arc::clone(grinder(&state)?);
    let dir = grinder.store().artifact_dir()?.ok_or_else(|| ApiError {
        error: ErrorKind::Unsupported,
        message: "no embeddings are available; import artifact's llm-search-index first".into(),
        code: Some("import_artifact"),
    })?;

    let limit = params.limit.clamp(1, MAX_RESULTS);
    let id = params.id;
    let page = params.page;
    let state_for_blocking = Arc::clone(&state);
    let hits = tokio::task::spawn_blocking(move || -> Result<Vec<SimilarHit>, String> {
        let similarity = state_for_blocking.similarity.get(&dir)?;
        let store = grinder.store();
        let passage = |work: i32, page: u16| -> Option<Vec<u8>> {
            let pages = store.text(work).ok().flatten()?;
            let text = pages.into_iter().find(|p| p.page == page)?.lines.concat();
            Some(tsuburu_dialogue::jamo::codes(&text))
        };
        // The passage is in the imported index, or only in what was read here.
        // Either way it supplies the query, and both sides are searched: a
        // work read on this machine still has the whole corpus to match
        // against.
        let in_index = similarity.stored_vector(id, page);
        let Some(query) = (match in_index {
            Some(vector) => Some(vector),
            None => store.vector(id, page).map_err(|e| e.to_string())?,
        }) else {
            return Err(format!("gallery {id} has no embedding to compare with"));
        };
        let from_index = if similarity.stored_vector(id, page).is_some() {
            // Its own chapters are excluded by work rather than by score.
            similarity.near(id, page, limit, &passage)?
        } else {
            similarity.near_vector(&query, limit, &passage)
        };
        let from_here = crate::similar::near_local(store, &query, limit, Some(id));
        let matches = crate::similar::merge(from_index, from_here, limit);
        Ok(matches
            .into_iter()
            .map(|m| SimilarHit {
                gallery_id: m.gallery_id,
                page: m.page,
                score: m.score,
                // The passage itself, straight from the imported corpus.
                snippet: store
                    .text(m.gallery_id)
                    .ok()
                    .flatten()
                    .and_then(|pages| {
                        pages
                            .into_iter()
                            .find(|p| p.page == m.page)
                            .map(|p| p.lines.into_iter().take(3).collect::<Vec<_>>())
                    })
                    .unwrap_or_default(),
            })
            .collect())
    })
    .await
    .map_err(|e| storage_error(&e))?
    .map_err(|message| ApiError { error: ErrorKind::Storage, message, code: None })?;

    Ok(Json(hits))
}

// --- phrase search, through a model the user supplies ---

/// Where phrases get turned into vectors. Stored so it survives a restart.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EmbedderSettings {
    pub url: String,
    pub model: String,
}

impl Default for EmbedderSettings {
    fn default() -> Self {
        let d = tsuburu_embed::embedder::EmbedderConfig::default();
        Self { url: d.url, model: d.model }
    }
}

fn embedder_settings(grinder: &Grinder) -> EmbedderSettings {
    grinder
        .store()
        .embedder()
        .ok()
        .flatten()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

pub async fn get_embedder(
    State(state): State<Arc<AppState>>,
) -> Result<Json<EmbedderSettings>, ApiError> {
    Ok(Json(embedder_settings(grinder(&state)?)))
}

pub async fn set_embedder(
    State(state): State<Arc<AppState>>,
    Json(settings): Json<EmbedderSettings>,
) -> Result<Json<EmbedderSettings>, ApiError> {
    let grinder = grinder(&state)?;
    if settings.url.trim().is_empty() {
        return Err(ApiError::bad_request("give the embedding server's URL"));
    }
    let json = serde_json::to_string(&settings).map_err(|e| storage_error(&e))?;
    grinder.store().set_embedder(&json)?;
    Ok(Json(settings))
}

/// Asks the user's embedding server for a vector.
async fn embed(
    state: &AppState,
    settings: &EmbedderSettings,
    text: &str,
    dims: usize,
) -> Result<Vec<f32>, ApiError> {
    let config = tsuburu_embed::embedder::EmbedderConfig {
        url: settings.url.clone(),
        model: settings.model.clone(),
        dims,
    };
    let body = tsuburu_embed::embedder::request_body(&config, text);
    let reply = state.fetcher.post_json(&config.url, &body).await.map_err(|err| ApiError {
        error: ErrorKind::Network,
        message: format!(
            "could not reach the embedding server at {} ({err}). Start one and point \
                 tsuburu at it.",
            config.url
        ),
        code: Some("embedder_unreachable"),
    })?;
    tsuburu_embed::embedder::parse_embedding(&reply, dims).map_err(|e| ApiError {
        error: ErrorKind::Network,
        message: e.to_string(),
        code: None,
    })
}

fn similarity_dir(grinder: &Grinder) -> Result<PathBuf, ApiError> {
    grinder.store().artifact_dir()?.ok_or_else(|| ApiError {
        error: ErrorKind::Unsupported,
        message: "no embeddings are available; import artifact's llm-search-index first".into(),
        code: None,
    })
}

#[derive(Debug, Deserialize)]
pub struct PhraseParams {
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

/// Scenes that mean something like the phrase given.
pub async fn phrase(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PhraseParams>,
) -> Result<Json<Vec<SimilarHit>>, ApiError> {
    let grinder = Arc::clone(grinder(&state)?);
    if params.q.trim().is_empty() {
        return Err(ApiError::bad_request("give a phrase to look for"));
    }
    let dir = similarity_dir(&grinder)?;
    let settings = embedder_settings(&grinder);
    let limit = params.limit.clamp(1, MAX_RESULTS);

    let dims = {
        let state = Arc::clone(&state);
        let dir = dir.clone();
        tokio::task::spawn_blocking(move || state.similarity.get(&dir).map(|s| s.dims()))
            .await
            .map_err(|e| storage_error(&e))?
            .map_err(|message| ApiError { error: ErrorKind::Storage, message, code: None })?
    };
    let query = embed(&state, &settings, &params.q, dims).await?;

    let hits = tokio::task::spawn_blocking(move || -> Result<Vec<SimilarHit>, String> {
        let similarity = state.similarity.get(&dir)?;
        let store = grinder.store();
        let passage = |work: i32, page: u16| -> Option<Vec<u8>> {
            let pages = store.text(work).ok().flatten()?;
            let text = pages.into_iter().find(|p| p.page == page)?.lines.concat();
            Some(tsuburu_dialogue::jamo::codes(&text))
        };
        let from_index = similarity.near_vector(&query, limit, &passage);
        let from_here = crate::similar::near_local(store, &query, limit, None);
        Ok(crate::similar::merge(from_index, from_here, limit)
            .into_iter()
            .map(|m| SimilarHit {
                gallery_id: m.gallery_id,
                page: m.page,
                score: m.score,
                snippet: store
                    .text(m.gallery_id)
                    .ok()
                    .flatten()
                    .and_then(|pages| {
                        pages
                            .into_iter()
                            .find(|p| p.page == m.page)
                            .map(|p| p.lines.into_iter().take(3).collect::<Vec<_>>())
                    })
                    .unwrap_or_default(),
            })
            .collect())
    })
    .await
    .map_err(|e| storage_error(&e))?
    .map_err(|message| ApiError { error: ErrorKind::Storage, message, code: None })?;

    Ok(Json(hits))
}

#[derive(Debug, Serialize)]
pub struct PackCheck {
    pub ok: bool,
    /// How close the server's vector for a known passage is to the stored one.
    pub cosine: f32,
    pub sample_gallery: i32,
    pub sample_page: u16,
    pub note: String,
}

/// Checks the user's model really is the one that built the index.
///
/// A passage already in the corpus is sent to the server and the answer
/// compared with the vector stored for it. Anything near 1 means the pack is
/// right; a low number means a different model, or a different way of
/// prompting it, and phrase search would return nonsense that looks fine.
#[derive(Debug, Deserialize)]
pub struct StoredParams {
    /// Only what reading produced. That is the part the reader did not ask
    /// for explicitly, so it is the part worth offering to delete.
    #[serde(default = "yes")]
    pub reading_only: bool,
    #[serde(default = "default_stored_limit")]
    pub limit: usize,
}

fn yes() -> bool {
    true
}

fn default_stored_limit() -> usize {
    50
}

#[derive(Debug, Serialize)]
pub struct StoredResponse {
    pub items: Vec<tsuburu_dialogue::Stored>,
    /// Everything matching, not just the page returned.
    pub total: usize,
    pub bytes: u64,
    /// Passages that also have an embedding, so meaning search reaches them.
    pub vectors: u64,
}

/// What this machine is keeping, newest first.
pub async fn stored(
    State(state): State<Arc<AppState>>,
    Query(params): Query<StoredParams>,
) -> Result<Json<StoredResponse>, ApiError> {
    let grinder = Arc::clone(grinder(&state)?);
    let limit = params.limit.clamp(1, 500);
    let (all, vectors) = tokio::task::spawn_blocking(move || {
        let store = grinder.store();
        Ok::<_, tsuburu_dialogue::DialogueError>((
            store.stored(params.reading_only, usize::MAX)?,
            store.vector_count()?,
        ))
    })
    .await
    .map_err(|e| storage_error(&e))??;

    let total = all.len();
    let bytes = all.iter().map(|s| s.bytes).sum();
    Ok(Json(StoredResponse { items: all.into_iter().take(limit).collect(), total, bytes, vectors }))
}

/// Forgets one gallery's text.
pub async fn forget(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let grinder = Arc::clone(grinder(&state)?);
    let removed = tokio::task::spawn_blocking(move || grinder.store().forget(id))
        .await
        .map_err(|e| storage_error(&e))??;
    Ok(Json(serde_json::json!({ "removed": removed })))
}

/// Forgets everything reading produced.
pub async fn forget_read(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let grinder = Arc::clone(grinder(&state)?);
    let removed = tokio::task::spawn_blocking(move || grinder.store().forget_read())
        .await
        .map_err(|e| storage_error(&e))??;
    Ok(Json(serde_json::json!({ "removed": removed })))
}

pub async fn check_pack(State(state): State<Arc<AppState>>) -> Result<Json<PackCheck>, ApiError> {
    let grinder = Arc::clone(grinder(&state)?);
    let dir = similarity_dir(&grinder)?;
    let settings = embedder_settings(&grinder);

    // Any indexed gallery will do; take the first one the corpus has.
    let sample = grinder.store().done_ids()?.into_iter().min().ok_or_else(|| {
        ApiError::bad_request("the corpus is empty, so there is nothing to check")
            .coded("corpus_empty")
    })?;
    let pages = grinder
        .store()
        .text(sample)?
        .ok_or_else(|| ApiError::bad_request("that gallery has no stored text"))?;
    let page = pages.first().cloned().ok_or_else(|| ApiError::bad_request("no pages"))?;
    let text = page.lines.join("\n");

    let (dims, stored) = {
        let state = Arc::clone(&state);
        let dir = dir.clone();
        let page_no = page.page;
        tokio::task::spawn_blocking(move || {
            state.similarity.get(&dir).map(|s| (s.dims(), s.stored_vector(sample, page_no)))
        })
        .await
        .map_err(|e| storage_error(&e))?
        .map_err(|message| ApiError { error: ErrorKind::Storage, message, code: None })?
    };
    let stored = stored.ok_or_else(|| ApiError::bad_request("that passage is not in the index"))?;

    let fresh = embed(&state, &settings, &text, dims).await?;
    let cosine = tsuburu_embed::embedder::cosine(&fresh, &stored);
    let ok = cosine > 0.9;
    Ok(Json(PackCheck {
        ok,
        cosine,
        sample_gallery: sample,
        sample_page: page.page,
        note: if ok {
            "the server's vectors line up with the index".into()
        } else {
            "the vectors do not line up: this is a different model, or it needs a different \
             prompt. Phrase search would return plausible-looking nonsense."
                .into()
        },
    }))
}
