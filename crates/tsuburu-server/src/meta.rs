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

const MAX_LIMIT: usize = 100;

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_id: Option<i32>,
}

pub async fn status(State(state): State<Arc<AppState>>) -> Result<Json<MetaStatus>, ApiError> {
    let Some(meta) = state.meta.as_ref() else {
        return Ok(Json(MetaStatus { available: false, works: None, latest_id: None }));
    };
    Ok(Json(MetaStatus {
        available: true,
        works: Some(meta.count().map_err(storage)?),
        latest_id: meta.latest_id().map_err(storage)?,
    }))
}

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    /// Free text: matched as a title substring and, whole, as an artist,
    /// series, character, group or tag. Results are the union.
    #[serde(default)]
    pub q: String,
    /// Free text matched against titles only (spacing-insensitive, Korean aware).
    #[serde(default)]
    pub title: String,
    /// Space separated terms; `artist:keso`, `tag:glasses`, or bare words.
    #[serde(default)]
    pub terms: String,
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
    let base = MetaQuery {
        title: Some(params.title.trim().to_string()).filter(|t| !t.is_empty()),
        terms: split_terms(&params.terms),
        language,
        kind,
        exists_only: true,
    };
    if base.is_empty() && free.is_empty() {
        return Err(ApiError::bad_request("give a title, a term, or a filter"));
    }
    let limit = params.limit.clamp(1, MAX_LIMIT);
    let offset = params.offset;
    let page = tokio::task::spawn_blocking(
        move || -> Result<tsuburu_meta::Page, tsuburu_meta::MetaError> {
            if free.is_empty() {
                return meta.search(&base, offset, limit);
            }
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

/// Terms are split on whitespace, except that a namespaced term keeps its
/// spaces: `tag:big breasts artist:keso` -> ["tag:big breasts", "artist:keso"].
fn split_terms(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in s.split_whitespace() {
        let namespaced = word.contains(':');
        match out.last_mut() {
            Some(last) if !namespaced && last.contains(':') => {
                last.push(' ');
                last.push_str(word);
            }
            _ => out.push(word.to_string()),
        }
    }
    out
}

#[derive(Debug, Deserialize)]
pub struct SuggestParams {
    pub prefix: String,
    #[serde(default = "default_suggest_limit")]
    pub limit: usize,
}

fn default_suggest_limit() -> usize {
    10
}

#[derive(Debug, Serialize)]
pub struct Suggestion {
    pub key: String,
    pub count: usize,
}

pub async fn suggest(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SuggestParams>,
) -> Result<Json<Vec<Suggestion>>, ApiError> {
    let meta = meta(&state)?;
    let items = meta.suggest(&params.prefix, params.limit.clamp(1, 50)).map_err(storage)?;
    Ok(Json(items.into_iter().map(|(key, count)| Suggestion { key, count }).collect()))
}

fn storage(err: impl std::fmt::Display) -> ApiError {
    ApiError { error: ErrorKind::Storage, message: err.to_string(), code: None }
}

#[cfg(test)]
mod tests {
    use super::split_terms;

    #[test]
    fn namespaced_terms_keep_their_spaces() {
        assert_eq!(
            split_terms("tag:big breasts artist:keso glasses"),
            vec!["tag:big breasts", "artist:keso glasses"]
        );
        assert_eq!(split_terms("keso glasses"), vec!["keso", "glasses"]);
    }
}
