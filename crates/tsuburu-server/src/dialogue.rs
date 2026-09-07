//! Dialogue search API.
//!
//! Every response says how much of the corpus is covered. A miss has to be
//! readable as "not indexed yet" rather than "does not exist", or the user
//! cannot trust the feature.

use axum::Json;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tsuburu_dialogue::{Counts, Hit, Priority};

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
