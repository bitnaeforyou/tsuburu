//! 즐겨찾기와 읽음 기록.
//!
//! 저장소를 열지 못했더라도 검색은 계속 동작해야 한다(스펙 7절). 그래서
//! [`AppState::store`]는 `Option`이고, 여기서는 없을 때 명확한 오류를 낸다.

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tsuburu_store::{Favorite, HistoryEntry, Store, Summary};

use crate::error::{ApiError, ErrorKind};
use crate::state::AppState;

fn store(state: &AppState) -> Result<&Store, ApiError> {
    state.store.as_ref().ok_or_else(|| ApiError {
        error: ErrorKind::Storage,
        message: "the local library is unavailable, so favorites and history are disabled".into(),
        code: Some("no_library"),
    })
}

#[derive(Debug, Serialize)]
pub struct FavoritesResponse {
    pub items: Vec<Favorite>,
}

pub async fn list_favorites(
    State(state): State<Arc<AppState>>,
) -> Result<Json<FavoritesResponse>, ApiError> {
    Ok(Json(FavoritesResponse { items: store(&state)?.favorites()? }))
}

/// 카드 요약을 함께 받는다. 목록을 그릴 때 갤러리 메타를 다시 받지 않기 위한
/// 것이다(스펙 5.1절).
#[derive(Debug, Deserialize)]
pub struct SummaryBody {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub pages: usize,
    #[serde(default)]
    pub thumbnail_hash: Option<String>,
}

impl SummaryBody {
    fn into_summary(self, id: i32) -> Summary {
        Summary {
            id,
            title: self.title,
            language: self.language,
            kind: self.kind,
            pages: self.pages,
            thumbnail_hash: self.thumbnail_hash,
        }
    }
}

pub async fn add_favorite(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
    Json(body): Json<SummaryBody>,
) -> Result<Json<Favorite>, ApiError> {
    Ok(Json(store(&state)?.add_favorite(body.into_summary(id))?))
}

#[derive(Debug, Serialize)]
pub struct Removed {
    pub removed: bool,
}

pub async fn remove_favorite(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
) -> Result<Json<Removed>, ApiError> {
    Ok(Json(Removed { removed: store(&state)?.remove_favorite(id)? }))
}

#[derive(Debug, Serialize)]
pub struct HistoryResponse {
    pub items: Vec<HistoryEntry>,
}

pub async fn list_history(
    State(state): State<Arc<AppState>>,
) -> Result<Json<HistoryResponse>, ApiError> {
    Ok(Json(HistoryResponse { items: store(&state)?.history()? }))
}

#[derive(Debug, Deserialize)]
pub struct ProgressBody {
    pub page: usize,
    #[serde(flatten)]
    pub summary: SummaryBody,
}

pub async fn record_progress(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
    Json(body): Json<ProgressBody>,
) -> Result<Json<HistoryEntry>, ApiError> {
    let page = body.page;
    Ok(Json(store(&state)?.record_progress(body.summary.into_summary(id), page)?))
}

pub async fn clear_history(State(state): State<Arc<AppState>>) -> Result<Json<Removed>, ApiError> {
    store(&state)?.clear_history()?;
    Ok(Json(Removed { removed: true }))
}
