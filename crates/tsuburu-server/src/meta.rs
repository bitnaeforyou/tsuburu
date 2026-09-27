//! Search over the local metadata snapshot.
//!
//! Korean titles, artists, series and characters are all findable here
//! without the network, which the remote index cannot do (it only knows
//! romanised names and hitomi's own tags).

use axum::Json;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tsuburu_meta::MetaQuery;

use crate::error::{ApiError, ErrorKind};
use crate::state::AppState;

use crate::MAX_LIMIT;

fn meta(state: &AppState) -> Result<&Arc<tsuburu_meta::MetaStore>, ApiError> {
    state.meta.as_ref().ok_or_else(|| ApiError {
        error: ErrorKind::Unsupported,
        message: "no metadata snapshot has been imported (tsuburu import-meta <data.db>)".into(),
        code: Some("import_meta"),
    })
}

#[derive(Debug, Serialize)]
pub struct MetaStatus {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub works: Option<usize>,
}

pub async fn status(State(state): State<Arc<AppState>>) -> Result<Json<MetaStatus>, ApiError> {
    let Some(meta) = state.meta.as_ref() else {
        return Ok(Json(MetaStatus { available: false, works: None }));
    };
    Ok(Json(MetaStatus { available: true, works: Some(meta.count().map_err(storage)?) }))
}

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    /// Free text: matched as a title substring and, whole, as an artist,
    /// series, character, group or tag. Results are the union.
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
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
    pub total: usize,
    pub ids: Vec<i32>,
}

pub async fn search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Result<Json<SearchResponse>, ApiError> {
    let meta = Arc::clone(meta(&state)?);
    let language = params.language.filter(|s| !s.is_empty() && s != "all");
    let kind = params.kind.filter(|s| !s.is_empty() && s != "all");
    let free = params.q.trim().to_string();
    let base = MetaQuery { language, kind, exists_only: true, ..MetaQuery::default() };
    if base.is_empty() && free.is_empty() {
        return Err(ApiError::bad_request("give something to look for, or a filter"));
    }
    let limit = params.limit.clamp(1, MAX_LIMIT);
    let offset = params.offset;
    let page = tokio::task::spawn_blocking(
        move || -> Result<tsuburu_meta::Page, tsuburu_meta::MetaError> {
            // Free text: title substring, or the whole text as one term.
            let by_title = meta.search(
                &MetaQuery { title: Some(free.clone()), ..base.clone() },
                0,
                usize::MAX,
            )?;
            let by_term = meta.search(
                &MetaQuery { terms: vec![free.clone()], ..base.clone() },
                0,
                usize::MAX,
            )?;
            let mut ids: Vec<i32> = by_title.ids;
            let seen: std::collections::HashSet<i32> = ids.iter().copied().collect();
            ids.extend(by_term.ids.into_iter().filter(|id| !seen.contains(id)));
            ids.sort_unstable_by(|a, b| b.cmp(a));
            let total = ids.len();
            Ok(tsuburu_meta::Page {
                total,
                ids: ids.into_iter().skip(offset).take(limit).collect(),
            })
        },
    )
    .await
    .map_err(|e| storage(&e))?
    .map_err(storage)?;
    Ok(Json(SearchResponse { total: page.total, ids: page.ids }))
}

fn storage(err: impl std::fmt::Display) -> ApiError {
    ApiError { error: ErrorKind::Storage, message: err.to_string(), code: None }
}
