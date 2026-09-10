//! Artists: their works, and the ones being followed.
//!
//! The metadata snapshot indexes every artist, so a name resolves to a work
//! list without the network. Following is kept in the library beside
//! favorites, since it is the same kind of thing: something the user chose
//! to keep.

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tsuburu_meta::MetaQuery;

use crate::error::{ApiError, ErrorKind};
use crate::state::AppState;

const MAX_LIMIT: usize = 100;

fn library(state: &AppState) -> Result<&tsuburu_store::Store, ApiError> {
    state.store.as_ref().ok_or_else(|| ApiError {
        error: ErrorKind::Storage,
        message: "the local library is unavailable, so following is disabled".into(),
        code: Some("no_library"),
    })
}

fn meta(state: &AppState) -> Result<&Arc<tsuburu_meta::MetaStore>, ApiError> {
    state.meta.as_ref().ok_or_else(|| ApiError {
        error: ErrorKind::Unsupported,
        message: "no metadata snapshot has been imported (tsuburu import-meta <data.db>)".into(),
        code: Some("import_meta"),
    })
}

#[derive(Debug, Deserialize)]
pub struct WorksParams {
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub language: Option<String>,
}

fn default_limit() -> usize {
    25
}

#[derive(Debug, Serialize)]
pub struct ArtistResponse {
    pub name: String,
    pub total: usize,
    pub ids: Vec<i32>,
    pub following: bool,
    /// How many works the snapshot has per language, most first.
    pub languages: Vec<(String, usize)>,
}

/// One artist's works, newest first.
pub async fn works(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(params): Query<WorksParams>,
) -> Result<Json<ArtistResponse>, ApiError> {
    let meta = Arc::clone(meta(&state)?);
    let name = name.trim().to_lowercase();
    if name.is_empty() {
        return Err(ApiError::bad_request("give an artist name"));
    }
    let limit = params.limit.clamp(1, MAX_LIMIT);
    let offset = params.offset;
    let language = params.language.filter(|l| !l.is_empty() && l != "all");

    let query = MetaQuery {
        terms: vec![format!("artist:{name}")],
        language: language.clone(),
        exists_only: true,
        ..MetaQuery::default()
    };
    let query_for_page = query.clone();
    // The language breakdown is what tells a reader whether an artist is
    // worth following in the language they read, so it is counted over the
    // whole catalogue - which is why this runs off the runtime's threads.
    let every_language = MetaQuery { language: None, ..query };
    let wants_languages = offset == 0;
    let (page, languages) = tokio::task::spawn_blocking(move || {
        let page = meta.search(&query_for_page, offset, limit)?;
        if !wants_languages {
            return Ok((page, Vec::new()));
        }
        let all = meta.search(&every_language, 0, usize::MAX)?;
        let mut counts: std::collections::HashMap<String, usize> = Default::default();
        for work in meta.works(&all.ids)? {
            *counts.entry(work.language).or_default() += 1;
        }
        let mut counts: Vec<(String, usize)> =
            counts.into_iter().filter(|(l, _)| !l.is_empty()).collect();
        counts.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        counts.truncate(8);
        Ok::<_, tsuburu_meta::MetaError>((page, counts))
    })
    .await
    .map_err(|e| storage(&e))?
    .map_err(storage)?;

    let following =
        state.store.as_ref().and_then(|s| s.follows_artist(&name).ok()).unwrap_or(false);
    Ok(Json(ArtistResponse { name, total: page.total, ids: page.ids, following, languages }))
}

#[derive(Debug, Serialize)]
pub struct Followed {
    pub name: String,
    /// How many of their works the snapshot knows.
    pub works: usize,
    /// The newest few, for a glance.
    pub recent: Vec<i32>,
}

pub async fn following(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Followed>>, ApiError> {
    let names = library(&state)?.followed_artists()?;
    let Some(meta) = state.meta.as_ref() else {
        return Ok(Json(
            names.into_iter().map(|name| Followed { name, works: 0, recent: Vec::new() }).collect(),
        ));
    };
    // One term lookup per followed name; enough of them to keep off the
    // runtime's threads.
    let meta = Arc::clone(meta);
    let out = tokio::task::spawn_blocking(move || {
        let mut out = Vec::with_capacity(names.len());
        for name in names {
            let page = meta.search(
                &MetaQuery {
                    terms: vec![format!("artist:{name}")],
                    exists_only: true,
                    ..MetaQuery::default()
                },
                0,
                6,
            )?;
            out.push(Followed { name, works: page.total, recent: page.ids });
        }
        Ok::<_, tsuburu_meta::MetaError>(out)
    })
    .await
    .map_err(|e| storage(&e))?
    .map_err(storage)?;
    Ok(Json(out))
}

#[derive(Debug, Serialize)]
pub struct FollowResponse {
    pub following: bool,
}

pub async fn follow(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<FollowResponse>, ApiError> {
    let followed = library(&state)?.follow_artist(&name)?;
    if !followed {
        return Err(ApiError::bad_request("give an artist name"));
    }
    Ok(Json(FollowResponse { following: true }))
}

pub async fn unfollow(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<FollowResponse>, ApiError> {
    library(&state)?.unfollow_artist(&name)?;
    Ok(Json(FollowResponse { following: false }))
}

fn storage(err: impl std::fmt::Display) -> ApiError {
    ApiError { error: ErrorKind::Storage, message: err.to_string(), code: None }
}
