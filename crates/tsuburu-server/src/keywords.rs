//! What a work is about, and what else is about the same thing.
//!
//! These come from a published `graph.csv`, which is a few tens of megabytes
//! rather than the embedding index's 2.6 GB. The answers are coarser - shared
//! words, not shared meaning - but they cost nothing to keep.

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::{ApiError, ErrorKind};
use crate::state::AppState;

const MAX_RESULTS: usize = 100;

fn store(state: &AppState) -> Result<&Arc<tsuburu_keywords::KeywordStore>, ApiError> {
    state.keywords.as_ref().ok_or_else(|| ApiError {
        error: ErrorKind::Unsupported,
        message: "no keywords have been imported (tsuburu import-keywords <graph.csv>)".into(),
        code: Some("import_keywords"),
    })
}

fn storage(err: impl std::fmt::Display) -> ApiError {
    ApiError { error: ErrorKind::Storage, message: err.to_string(), code: None }
}

#[derive(Debug, Serialize)]
pub struct Word {
    pub word: String,
    pub score: f32,
}

#[derive(Debug, Serialize)]
pub struct WordsResponse {
    pub id: i32,
    pub words: Vec<Word>,
}

/// The words one work is about, strongest first.
pub async fn of(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
) -> Result<Json<WordsResponse>, ApiError> {
    let store = Arc::clone(store(&state)?);
    let words = tokio::task::spawn_blocking(move || store.of(id))
        .await
        .map_err(|e| storage(&e))?
        .map_err(storage)?
        .unwrap_or_default();
    Ok(Json(WordsResponse {
        id,
        words: words.into_iter().map(|w| Word { word: w.word, score: w.score }).collect(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct NearParams {
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    12
}

#[derive(Debug, Serialize)]
pub struct Near {
    pub id: i32,
    pub score: f32,
    /// The words the two works have in common, heaviest first.
    pub shared: Vec<String>,
}

/// Works about the same things as this one.
pub async fn near(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
    Query(params): Query<NearParams>,
) -> Result<Json<Vec<Near>>, ApiError> {
    let store = Arc::clone(store(&state)?);
    let limit = params.limit.clamp(1, MAX_RESULTS);
    let found = tokio::task::spawn_blocking(move || store.near(id, limit))
        .await
        .map_err(|e| storage(&e))?
        .map_err(storage)?;
    Ok(Json(
        found.into_iter().map(|n| Near { id: n.id, score: n.score, shared: n.shared }).collect(),
    ))
}

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    pub q: String,
    #[serde(default = "default_search_limit")]
    pub limit: usize,
}

fn default_search_limit() -> usize {
    25
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub word: String,
    pub works: Vec<Hit>,
    /// How many works held this word, when it was left out of the index for
    /// being in too many of them. A reader who gets nothing back deserves to
    /// know the difference between "nowhere" and "everywhere".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub too_common: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct Hit {
    pub id: i32,
    pub score: f32,
}

/// Works this word belongs to most strongly.
pub async fn search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Result<Json<SearchResponse>, ApiError> {
    let store = Arc::clone(store(&state)?);
    let word = params.q.trim().to_string();
    if word.is_empty() {
        return Err(ApiError::bad_request("give a word to look for"));
    }
    let limit = params.limit.clamp(1, MAX_RESULTS);
    let looked = word.clone();
    let (works, too_common) = tokio::task::spawn_blocking(move || {
        let works = store.search(&looked, limit)?;
        // Only worth asking when the answer is empty.
        let too_common = if works.is_empty() { store.common(&looked)? } else { None };
        Ok::<_, tsuburu_keywords::KeywordError>((works, too_common))
    })
    .await
    .map_err(|e| storage(&e))?
    .map_err(storage)?;
    Ok(Json(SearchResponse {
        word,
        works: works.into_iter().map(|(id, score)| Hit { id, score }).collect(),
        too_common,
    }))
}
