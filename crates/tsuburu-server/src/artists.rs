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

/// The languages an artist's page offers to narrow by, when the count has to
/// come from hitomi rather than the snapshot. One HEAD each, and only the
/// ones a reader is offered in the first place.
const LANGUAGES: [&str; 5] = ["korean", "japanese", "english", "chinese", "spanish"];

/// One artist's or one series' works, from hitomi's own lists.
///
/// Used when there is no snapshot to ask, which is every phone: hitomi keeps
/// a list per name per language beside its search index, so the page works
/// with nothing imported.
async fn from_hitomi(
    state: &AppState,
    namespace: &str,
    name: &str,
    params: &WorksParams,
    limit: usize,
) -> Result<(usize, Vec<i32>, Vec<(String, usize)>), ApiError> {
    let language = params.language.as_deref().filter(|l| !l.is_empty()).unwrap_or("all");
    let url = state.cfg.name_list_url(namespace, name, language);
    let fetcher = state.fetcher.as_ref();
    let total = tsuburu_hitomi::nozomi::count(fetcher, &url).await.unwrap_or(0);
    if total == 0 {
        return Ok((0, Vec::new(), Vec::new()));
    }
    let ids = tsuburu_hitomi::nozomi::page(fetcher, &url, params.offset, limit).await?;

    let mut languages = Vec::new();
    if params.offset == 0 {
        for one in LANGUAGES {
            let url = state.cfg.name_list_url(namespace, name, one);
            if let Ok(n) = tsuburu_hitomi::nozomi::count(fetcher, &url).await
                && n > 0
            {
                languages.push((one.to_string(), n));
            }
        }
        languages.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    }
    Ok((total, ids, languages))
}

/// One series' works. Only hitomi indexes these by name.
pub async fn series(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(params): Query<WorksParams>,
) -> Result<Json<ArtistResponse>, ApiError> {
    let name = name.trim().to_lowercase();
    if name.is_empty() {
        return Err(ApiError::bad_request("give a series name"));
    }
    let limit = params.limit.clamp(1, MAX_LIMIT);
    let (total, ids, languages) = from_hitomi(&state, "series", &name, &params, limit).await?;
    Ok(Json(ArtistResponse { name, total, ids, following: false, languages }))
}

/// One artist's works, newest first.
pub async fn works(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(params): Query<WorksParams>,
) -> Result<Json<ArtistResponse>, ApiError> {
    let name = name.trim().to_lowercase();
    if name.is_empty() {
        return Err(ApiError::bad_request("give an artist name"));
    }
    let limit = params.limit.clamp(1, MAX_LIMIT);
    let following =
        state.store.as_ref().and_then(|s| s.follows_artist(&name).ok()).unwrap_or(false);

    // Tapping an artist used to be a dead end wherever no snapshot had been
    // imported, which is every phone. hitomi lists them itself.
    let Some(meta) = state.meta.as_ref() else {
        let (total, ids, languages) = from_hitomi(&state, "artist", &name, &params, limit).await?;
        return Ok(Json(ArtistResponse { name, total, ids, following, languages }));
    };
    let meta = Arc::clone(meta);
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
