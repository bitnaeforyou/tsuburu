//! JSON API.
//!
//! 브라우저는 hitomi를 직접 보지 않는다. 여기서 내려가는 것은 카드 한 장에
//! 수백 바이트짜리 요약과 프록시 경로뿐이다.

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::ApiError;
use crate::state::AppState;

/// 한 번에 그릴 수 있는 결과 수. 상한을 두지 않으면 클라이언트가 수천 개를
/// 한꺼번에 요청해 hitomi에 그대로 부하가 간다.
const MAX_LIMIT: usize = 100;
const MAX_CARDS: usize = 50;

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
    pub total: usize,
    pub ids: Vec<i32>,
}

pub async fn search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Result<Json<SearchResponse>, ApiError> {
    let query = tsuburu_hitomi::parse_query(&params.q);
    if query.include.is_empty() {
        return Err(ApiError::bad_request(
            "give at least one search term that is not an exclusion",
        ));
    }
    let limit = params.limit.clamp(1, MAX_LIMIT);

    let version = state.version().await?;
    let page = tsuburu_hitomi::search_page(
        state.fetcher.as_ref(),
        &state.cfg,
        &version,
        &query,
        params.offset,
        limit,
    )
    .await?;

    Ok(Json(SearchResponse { total: page.total, ids: page.ids }))
}

/// 결과 그리드에 그릴 최소 정보.
#[derive(Debug, Clone, Serialize)]
pub struct Card {
    pub id: i32,
    pub title: Option<String>,
    pub kind: Option<String>,
    pub language: Option<String>,
    pub pages: usize,
    pub tags: Vec<String>,
    /// 이 서버의 썸네일 프록시 경로. 브라우저는 hitomi를 직접 보지 않는다.
    pub thumbnail: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CardsParams {
    /// 쉼표로 구분된 갤러리 ID 목록.
    pub ids: String,
}

pub async fn cards(
    State(state): State<Arc<AppState>>,
    Query(params): Query<CardsParams>,
) -> Result<Json<Vec<Card>>, ApiError> {
    let ids: Vec<i32> = params
        .ids
        .split(',')
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.trim().parse().ok())
        .take(MAX_CARDS)
        .collect();

    if ids.is_empty() {
        return Err(ApiError::bad_request("ids must contain at least one gallery id"));
    }

    let gg = state.gg().await?;
    let mut out = Vec::with_capacity(ids.len());

    // 갤러리 메타는 최대 200 KB를 넘기도 하므로 동시 요청 수는 fetcher의
    // 세마포어가 조인다. 여기서는 순서를 유지한 채 모으기만 한다.
    let mut tasks = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(cached) = state.cards.get(&id) {
            tasks.push(CardTask::Ready(cached));
            continue;
        }
        let state = Arc::clone(&state);
        let gg = gg.clone();
        tasks.push(CardTask::Pending(tokio::spawn(async move {
            build_card(&state, &gg, id).await
        })));
    }

    for task in tasks {
        match task {
            CardTask::Ready(card) => out.push(card),
            CardTask::Pending(handle) => match handle.await {
                Ok(Ok(card)) => {
                    state.cards.insert(card.id, card.clone());
                    out.push(card);
                }
                // 한 장이 실패했다고 페이지 전체를 버리지 않는다.
                Ok(Err(err)) => tracing::warn!(%err, "skipping a card"),
                Err(err) => tracing::warn!(%err, "card task panicked"),
            },
        }
    }

    Ok(Json(out))
}

enum CardTask {
    Ready(Card),
    Pending(tokio::task::JoinHandle<Result<Card, tsuburu_hitomi::GalleryFetchError>>),
}

async fn build_card(
    state: &AppState,
    gg: &tsuburu_hitomi::GgMap,
    id: i32,
) -> Result<Card, tsuburu_hitomi::GalleryFetchError> {
    let gallery = tsuburu_hitomi::fetch_gallery(state.fetcher.as_ref(), &state.cfg, id).await?;
    // URL을 실제로 만들어보고, 만들 수 있을 때만 경로를 내보낸다.
    let thumbnail = gallery.files.first().and_then(|f| {
        tsuburu_hitomi::thumbnail_url(&state.cfg, gg, &f.hash).ok()?;
        Some(format!("/tn/{}.avif", f.hash))
    });

    Ok(Card {
        id,
        title: gallery.title,
        kind: gallery.kind,
        language: gallery.language,
        pages: gallery.files.len(),
        tags: gallery.tags.into_iter().take(8).collect(),
        thumbnail,
    })
}

#[derive(Debug, Serialize)]
pub struct GalleryResponse {
    pub id: i32,
    pub title: Option<String>,
    pub japanese_title: Option<String>,
    pub kind: Option<String>,
    pub language: Option<String>,
    pub date: Option<String>,
    pub tags: Vec<String>,
    pub pages: Vec<Page>,
}

#[derive(Debug, Serialize)]
pub struct Page {
    /// 이 서버의 이미지 프록시 경로.
    pub src: String,
    pub width: u32,
    pub height: u32,
}

pub async fn gallery(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
) -> Result<Json<GalleryResponse>, ApiError> {
    let gallery = tsuburu_hitomi::fetch_gallery(state.fetcher.as_ref(), &state.cfg, id).await?;

    let pages = gallery
        .files
        .iter()
        .map(|f| Page {
            src: format!("/img/{}.{}", f.hash, tsuburu_hitomi::image::image_extension(f)),
            width: f.width,
            height: f.height,
        })
        .collect();

    Ok(Json(GalleryResponse {
        id,
        title: gallery.title,
        japanese_title: gallery.japanese_title,
        kind: gallery.kind,
        language: gallery.language,
        date: gallery.date,
        tags: gallery.tags,
        pages,
    }))
}
