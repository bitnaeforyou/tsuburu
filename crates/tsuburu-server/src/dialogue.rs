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
use tsuburu_embed::runner::Progress;

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
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    25
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub hits: Vec<Hit>,
    /// How many works said it, which is usually more than one page of them.
    pub total: usize,
    /// How far into that total a reader can actually go. Past it the phrase
    /// wants narrowing rather than paging.
    pub reachable: usize,
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
    let offset = params.offset;
    // The scan is CPU work over the whole store; keep it off the runtime.
    let store_grinder = Arc::clone(grinder);
    let query = params.q.clone();
    let found = tokio::task::spawn_blocking(move || {
        store_grinder.store().search_page(&query, offset, limit)
    })
    .await
    .map_err(|e| ApiError {
        error: ErrorKind::Storage,
        message: e.to_string(),
        code: None,
    })??;
    Ok(Json(SearchResponse {
        hits: found.hits,
        total: found.total,
        reachable: found.reachable,
        counts: grinder.store().counts()?,
    }))
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
            // Not the reader's hidden tags: this is naming outright what to
            // read, and what was named is what is meant.
            hidden: &Default::default(),
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
        grinder.store().export_shards(tsuburu_dialogue::SHARD_RANGE, body.background_only)
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

// --- what a phrase search hands back ---

#[derive(Debug, Serialize)]
pub struct SimilarHit {
    pub gallery_id: i32,
    pub page: u16,
    pub score: f32,
    pub snippet: Vec<String>,
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

fn stored_embedder(grinder: &Grinder) -> EmbedderSettings {
    grinder
        .store()
        .embedder()
        .ok()
        .flatten()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

/// Where to send a phrase to be turned into a vector.
///
/// The model tsuburu fetched and is running answers for itself; the stored
/// address is only for someone who would rather run their own.
/// Where to turn a phrase into a vector.
///
/// The model this program runs answers on a port it picks at the time, so it
/// is asked for its address rather than assumed. While it is coming up there
/// is no address to have - and falling back to whatever was saved before the
/// switch existed meant blaming the network for a port nobody chose. Once the
/// switch is on, the model is the answer or the reason there isn't one.
fn embedder_settings(state: &AppState, grinder: &Grinder) -> Result<EmbedderSettings, ApiError> {
    let mut settings = stored_embedder(grinder);
    if let Some(url) = state.model.url() {
        settings.url = url;
        return Ok(settings);
    }
    if state.model.wanted() {
        return Err(match state.model.progress() {
            Progress::Fetching { .. } => ApiError {
                error: ErrorKind::Unsupported,
                message: "the model is still downloading; this works once it is here".into(),
                code: Some("model_busy"),
            },
            Progress::Failed { error } => ApiError {
                error: ErrorKind::Network,
                message: format!("the model did not start: {error}"),
                code: Some("embedder_unreachable"),
            },
            _ => ApiError {
                error: ErrorKind::Unsupported,
                message: "the model is still starting up; try again in a moment".into(),
                code: Some("model_busy"),
            },
        });
    }
    Ok(settings)
}

pub async fn get_embedder(
    State(state): State<Arc<AppState>>,
) -> Result<Json<EmbedderSettings>, ApiError> {
    Ok(Json(stored_embedder(grinder(&state)?)))
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
            "could not reach the model at {} ({err}). Turn \"say it in your own words\" \
             off and back on in settings.",
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

/// The imported corpus, if there is one. Searching by a phrase does not need
/// it: the vectors for what this machine has read are searched the same way,
/// and somebody who has never imported anything still has those.
fn imported_index(grinder: &Grinder) -> Option<PathBuf> {
    grinder.store().artifact_dir().ok().flatten()
}

/// What the published corpus is doing, if anything.
#[derive(Serialize)]
pub struct CorpusState {
    /// False where the build does not know where it was published from, and
    /// so has nowhere to fetch from.
    pub available: bool,
    #[serde(flatten)]
    pub progress: crate::corpus::Progress,
}

pub async fn corpus_state(State(state): State<Arc<AppState>>) -> Json<CorpusState> {
    Json(CorpusState { available: state.corpus.publishable(), progress: state.corpus.progress() })
}

/// Fetches the published corpus, in the background.
pub async fn fetch_corpus(
    State(state): State<Arc<AppState>>,
) -> Result<Json<CorpusState>, ApiError> {
    let grinder = Arc::clone(grinder(&state)?);
    if !state.corpus.publishable() {
        return Err(ApiError {
            error: ErrorKind::Unsupported,
            message: "this build does not say where it was published from".into(),
            code: Some("unpublished"),
        });
    }
    state.corpus.fetch(grinder, Arc::clone(&state.fetcher));
    Ok(Json(CorpusState { available: true, progress: state.corpus.progress() }))
}

#[derive(Debug, Deserialize)]
pub struct PhraseParams {
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

/// Scenes that mean something like the phrase given.
/// How much text a page needs before it is worth offering as an answer.
///
/// Vectors for one or two words land near everything, so without this the
/// nearest passages to any phrase are the pages that say almost nothing.
const LEAST_TEXT: usize = 12;

pub async fn phrase(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PhraseParams>,
) -> Result<Json<Vec<SimilarHit>>, ApiError> {
    let grinder = Arc::clone(grinder(&state)?);
    if params.q.trim().is_empty() {
        return Err(ApiError::bad_request("give a phrase to look for"));
    }
    let dir = imported_index(&grinder);
    let settings = embedder_settings(&state, &grinder)?;
    let limit = params.limit.clamp(1, MAX_RESULTS);

    // Nothing to search at all is worth saying plainly: a reader who has just
    // switched the model on and read nothing yet is not looking at a fault.
    //
    // But one whose pages cannot be recognised at all is, and that reader has
    // been opening works and watching nothing happen. The sweep already knows
    // why; it is said here, where the emptiness is noticed.
    if dir.is_none() && grinder.store().vector_count().unwrap_or(0) == 0 {
        let blocked = grinder.status().await.last_error;
        return Err(ApiError {
            error: ErrorKind::Unsupported,
            message: match blocked {
                Some(why) => format!("nothing has been read yet, because: {why}"),
                None => "nothing has been read with the model on yet. Open a work and read \
                         it - its pages are recognised as you go, and this searches what \
                         they said."
                    .into(),
            },
            code: Some("nothing_embedded"),
        });
    }

    let dims = match &dir {
        Some(dir) => {
            let state = Arc::clone(&state);
            let dir = dir.clone();
            tokio::task::spawn_blocking(move || state.similarity.get(&dir).map(|s| s.dims()))
                .await
                .map_err(|e| storage_error(&e))?
                .map_err(|message| ApiError { error: ErrorKind::Storage, message, code: None })?
        }
        // What the model is asked for, which is what the stored vectors are.
        None => tsuburu_embed::embedder::EmbedderConfig::default().dims,
    };
    let query = embed(&state, &settings, &params.q, dims).await?;

    let hits = tokio::task::spawn_blocking(move || -> Result<Vec<SimilarHit>, String> {
        let store = grinder.store();
        let passage = |work: i32, page: u16| -> Option<Vec<u8>> {
            let pages = store.text(work).ok().flatten()?;
            let text = pages.into_iter().find(|p| p.page == page)?.lines.concat();
            Some(tsuburu_dialogue::jamo::codes(&text))
        };
        // Ask for more than will be shown: most of what comes back nearest a
        // phrase is a page whose whole text is "오빠" or "부글부글", and a
        // vector that short sits near everything. They are dropped below, and
        // dropping them from a list of exactly `limit` would leave gaps.
        let wide = (limit * 6).min(MAX_RESULTS * 6);
        let from_index = match &dir {
            Some(dir) => state.similarity.get(dir)?.near_vector(&query, wide, &passage),
            None => Vec::new(),
        };
        let from_here = crate::similar::near_local(store, &query, wide, None);
        Ok(crate::similar::merge(from_index, from_here, wide)
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
            .filter(|hit| {
                hit.snippet.iter().map(|line| line.chars().count()).sum::<usize>() >= LEAST_TEXT
            })
            .take(limit)
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

/// What the interface needs to draw one switch: what it is doing, whether the
/// weights are already here, and whether anybody publishes a model server for
/// this computer at all.
#[derive(Debug, Serialize)]
pub struct ModelState {
    #[serde(flatten)]
    pub progress: tsuburu_embed::runner::Progress,
    pub kept: bool,
    pub bytes: u64,
}

fn model_state(state: &AppState) -> ModelState {
    ModelState {
        progress: state.model.progress(),
        kept: state.model.kept(),
        bytes: state.model.bytes_kept(),
    }
}

pub async fn get_model(State(state): State<Arc<AppState>>) -> Json<ModelState> {
    Json(model_state(&state))
}

pub async fn enable_model(State(state): State<Arc<AppState>>) -> Json<ModelState> {
    state.model.enable();
    Json(model_state(&state))
}

pub async fn disable_model(State(state): State<Arc<AppState>>) -> Json<ModelState> {
    state.model.disable();
    Json(model_state(&state))
}

pub async fn check_pack(State(state): State<Arc<AppState>>) -> Result<Json<PackCheck>, ApiError> {
    let grinder = Arc::clone(grinder(&state)?);
    let dir = similarity_dir(&grinder)?;
    let settings = embedder_settings(&state, &grinder)?;

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
