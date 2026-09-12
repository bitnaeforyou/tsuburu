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
    /// `date`(기본), `today`, `week`, `month`, `year`.
    #[serde(default)]
    pub sort: Option<String>,
    /// `korean`, `japanese`, `english` 등. 없으면 전체.
    #[serde(default)]
    pub language: Option<String>,
    /// `doujinshi`, `manga`, `artistcg` 등. 없으면 전체.
    #[serde(default)]
    pub kind: Option<String>,
}

fn default_limit() -> usize {
    25
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub total: usize,
    pub ids: Vec<i32>,
    /// 입력의 각 낱말이 무엇으로 바뀌었는지. 화면이 이것을 그대로 보여준다.
    ///
    /// 한국어 사전은 태그를 잘 덮지만 작가 이름은 거의 덮지 못한다. 조용히
    /// 치환하면 작가 검색이 빈손으로 돌아올 때 "안 되는 기능"이 아니라
    /// "고장난 앱"으로 보인다(스펙 4.2절).
    pub terms: Vec<tsuburu_korean::Term>,
}

pub async fn search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Result<Json<SearchResponse>, ApiError> {
    // 한국어를 hitomi가 아는 영어로 보정한다. 사전에 없으면 입력 그대로 쓴다.
    let terms = tsuburu_korean::translate(tsuburu_korean::Dictionary::embedded(), &params.q);
    let query = tsuburu_hitomi::Query {
        include: terms.iter().filter(|t| !t.excluded).map(|t| t.used.to_lowercase()).collect(),
        exclude: terms.iter().filter(|t| t.excluded).map(|t| t.used.to_lowercase()).collect(),
    };

    let sort = match params.sort.as_deref() {
        None | Some("") => tsuburu_hitomi::Sort::Date,
        Some(name) => tsuburu_hitomi::Sort::parse(name)
            .ok_or_else(|| ApiError::bad_request(format!("unknown sort `{name}`")))?,
    };
    let filters = tsuburu_hitomi::Filters {
        language: params.language.filter(|s| !s.is_empty() && s != "all"),
        kind: params.kind.filter(|s| !s.is_empty() && s != "all"),
    };

    // 검색어가 없어도 필터나 정렬만으로 둘러볼 수 있다. 다만 제외어만 준 것은
    // 무엇을 빼야 할 대상인지가 없으므로 요청이 성립하지 않는다.
    if query.include.is_empty() && !query.exclude.is_empty() {
        return Err(ApiError::bad_request("exclusions need at least one term to exclude from"));
    }

    let limit = params.limit.clamp(1, MAX_LIMIT);
    let version = state.version().await?;
    let page = tsuburu_hitomi::search_page(
        state.fetcher.as_ref(),
        &state.cfg,
        &version,
        &tsuburu_hitomi::SearchRequest {
            query: &query,
            filters: &filters,
            sort,
            offset: params.offset,
            limit,
        },
    )
    .await?;

    Ok(Json(SearchResponse { total: page.total, ids: page.ids, terms }))
}

/// 결과 그리드에 그릴 최소 정보.
#[derive(Debug, Clone, Serialize)]
pub struct Card {
    pub id: i32,
    pub title: Option<String>,
    pub kind: Option<String>,
    pub language: Option<String>,
    pub pages: usize,
    /// Who drew it, where that is known: hitomi writes "N/A" when it is not,
    /// which is not a name and is not shown as one.
    pub artists: Vec<String>,
    pub tags: Vec<String>,
    /// 이 서버의 썸네일 프록시 경로. 브라우저는 hitomi를 직접 보지 않는다.
    pub thumbnail: Option<String>,
    /// Whether hitomi still lists it. `None` until the list has been read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listed: Option<bool>,
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

    // Reading hitomi's list of works is worth doing once and never worth
    // waiting for: the first page of results goes out without it.
    state.listing.warm(
        Arc::clone(&state.fetcher),
        state.cfg.sort_list_url(tsuburu_hitomi::Sort::Date, "all"),
    );

    let gg = state.gg().await?;
    let mut out = Vec::with_capacity(ids.len());

    // The local snapshot answers for anything it covers: no network, no
    // 200 KB gallery JSON per card.
    let snapshot: std::collections::HashMap<i32, Card> = match &state.meta {
        Some(meta) => meta
            .works(&ids)
            .unwrap_or_default()
            .into_iter()
            .map(|w| (w.id, card_from_work(w)))
            .collect(),
        None => Default::default(),
    };

    // 갤러리 메타는 최대 200 KB를 넘기도 하므로 동시 요청 수는 fetcher의
    // 세마포어가 조인다. 여기서는 순서를 유지한 채 모으기만 한다.
    let mut tasks = Vec::with_capacity(ids.len());
    for id in ids {
        // Older snapshot rows carry no thumbnail; their text is used as is
        // and the picture is fetched, falling back to text-only on failure.
        let fallback = snapshot.get(&id).cloned();
        if let Some(local) = &fallback
            && local.thumbnail.is_some()
        {
            tasks.push(CardTask::Ready(local.clone()));
            continue;
        }
        if let Some(cached) = state.cards.get(&id) {
            tasks.push(CardTask::Ready(cached));
            continue;
        }
        let state = Arc::clone(&state);
        let gg = gg.clone();
        tasks.push(CardTask::Pending(
            tokio::spawn(async move { build_card(&state, &gg, id).await }),
            fallback,
        ));
    }

    for task in tasks {
        match task {
            CardTask::Ready(card) => out.push(card),
            CardTask::Pending(handle, fallback) => match handle.await {
                Ok(Ok(card)) => {
                    state.cards.insert(card.id, card.clone());
                    out.push(card);
                }
                // 한 장이 실패했다고 페이지 전체를 버리지 않는다.
                Ok(Err(err)) => {
                    tracing::warn!(%err, "card fetch failed");
                    if let Some(card) = fallback {
                        out.push(card);
                    }
                }
                Err(err) => tracing::warn!(%err, "card task panicked"),
            },
        }
    }

    // Said once the list has been read, and not guessed at before then.
    for card in &mut out {
        card.listed = state.listing.listed(card.id);
    }

    Ok(Json(out))
}

/// The names among them, at most two: a result has room for that much.
fn named(artists: Vec<String>) -> Vec<String> {
    artists
        .into_iter()
        .filter(|name| !name.trim().is_empty() && !name.eq_ignore_ascii_case("N/A"))
        .take(2)
        .collect()
}

fn card_from_work(w: tsuburu_meta::Work) -> Card {
    Card {
        id: w.id,
        title: (!w.title.is_empty()).then_some(w.title),
        kind: (!w.kind.is_empty()).then_some(w.kind),
        language: (!w.language.is_empty()).then_some(w.language),
        pages: w.pages as usize,
        artists: named(w.artists),
        tags: w.tags.into_iter().take(8).collect(),
        thumbnail: w.thumbnail_hash.map(|h| format!("/tn/{h}.avif")),
        listed: None,
    }
}

#[cfg(test)]
mod card_tests {
    use super::named;

    #[test]
    fn keeps_the_names_and_drops_what_is_not_one() {
        assert_eq!(named(vec!["keso".into()]), vec!["keso".to_string()]);
        assert!(named(vec!["N/A".into()]).is_empty());
        assert!(named(vec!["n/a".into(), "  ".into()]).is_empty());
        assert_eq!(named(vec!["N/A".into(), "keso".into()]), vec!["keso".to_string()]);
    }

    #[test]
    fn a_result_has_room_for_two() {
        let many = vec!["a".into(), "b".into(), "c".into(), "d".into()];
        assert_eq!(named(many), vec!["a".to_string(), "b".to_string()]);
    }
}

enum CardTask {
    Ready(Card),
    Pending(tokio::task::JoinHandle<Result<Card, tsuburu_hitomi::GalleryFetchError>>, Option<Card>),
}

async fn build_card(
    state: &AppState,
    gg: &tsuburu_hitomi::GgMap,
    id: i32,
) -> Result<Card, tsuburu_hitomi::GalleryFetchError> {
    let gallery = tsuburu_hitomi::fetch_gallery(state.fetcher.as_ref(), &state.cfg, id).await?;
    remember(state, id, &gallery);
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
        artists: named(gallery.artists),
        tags: gallery.tags.into_iter().take(8).collect(),
        thumbnail,
        listed: None,
    })
}

/// The reader's view of a work that only exists on disk.
fn from_download(stored: tsuburu_downloads::Download) -> GalleryResponse {
    GalleryResponse {
        id: stored.id,
        title: stored.title,
        japanese_title: None,
        kind: stored.kind,
        language: stored.language,
        date: None,
        tags: Vec::new(),
        artists: Vec::new(),
        series: Vec::new(),
        pages: stored
            .pages
            .into_iter()
            .map(|p| Page {
                src: format!("/img/{}.{}", p.hash, p.ext),
                width: p.width,
                height: p.height,
            })
            .collect(),
    }
}

/// Notes which of a work's pages have no text yet.
///
/// The reader is about to pull those pages through the image proxy, and the
/// proxy can hand what it is already carrying to recognition. Pages that are
/// already stored are left out, so re-reading a work costs nothing.
async fn expect_unread_pages(state: &AppState, id: i32, gallery: &tsuburu_hitomi::Gallery) {
    let Some(grinder) = state.grinder.as_ref() else { return };
    if !grinder.settings().await.read_indexing {
        return;
    }
    let known: std::collections::HashSet<u16> = grinder
        .store()
        .text(id)
        .ok()
        .flatten()
        .map(|pages| pages.into_iter().map(|p| p.page).collect())
        .unwrap_or_default();
    let language = gallery.language.clone().unwrap_or_default();
    let waiting = gallery.files.iter().enumerate().filter_map(|(at, file)| {
        let page = u16::try_from(at).ok()?;
        (!known.contains(&page)).then(|| {
            (
                file.hash.clone(),
                crate::state::UnreadPage { gallery: id, page, language: language.clone() },
            )
        })
    });
    state.expect_pages(waiting).await;
}

/// Files a gallery fetched from hitomi into the local snapshot.
///
/// Failing to write is not worth failing the request over; the snapshot is
/// an optimisation, and the response is already in hand.
fn remember(state: &AppState, id: i32, gallery: &tsuburu_hitomi::Gallery) {
    let Some(meta) = state.meta.as_ref() else { return };
    let work = tsuburu_meta::Work {
        id,
        title: gallery.title.clone().unwrap_or_default(),
        kind: gallery.kind.clone().unwrap_or_default().to_lowercase().replace(' ', ""),
        language: gallery.language.clone().unwrap_or_default().to_lowercase(),
        artists: gallery.artists.clone(),
        groups: gallery.groups.clone(),
        series: gallery.series.clone(),
        characters: gallery.characters.clone(),
        tags: gallery.tags.clone(),
        published: None,
        pages: gallery.files.len() as u32,
        thumbnail_hash: gallery.files.first().map(|f| f.hash.clone()),
        exists: true,
    };
    if let Err(err) = meta.upsert(&work) {
        tracing::debug!(id, %err, "could not add the gallery to the snapshot");
    }
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
    pub artists: Vec<String>,
    pub series: Vec<String>,
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
    let gallery = match tsuburu_hitomi::fetch_gallery(state.fetcher.as_ref(), &state.cfg, id).await
    {
        Ok(gallery) => gallery,
        // A downloaded work must open with hitomi unreachable; its page list
        // is on disk, which is all the reader needs.
        Err(err) => {
            return match crate::downloads::stored_gallery(&state, id) {
                Some(stored) => Ok(Json(from_download(stored))),
                None => Err(err.into()),
            };
        }
    };
    remember(&state, id, &gallery);
    expect_unread_pages(&state, id, &gallery).await;

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
        artists: gallery.artists,
        series: gallery.series,
        pages,
    }))
}
