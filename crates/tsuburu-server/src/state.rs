//! 서버가 들고 있는 공유 상태.
//!
//! 인덱스 버전과 `gg.js`는 요청마다 받아오면 왕복이 두 배가 된다. 둘 다 자주
//! 바뀌지 않으므로 TTL을 두고 캐시한다.

use quick_cache::sync::Cache;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tsuburu_fetch::HttpFetcher;
use tsuburu_hitomi::{Config, GgMap};

use crate::api::Card;

const TTL: Duration = Duration::from_secs(60 * 60);

struct Cached<T> {
    value: T,
    fetched_at: Instant,
}

impl<T> Cached<T> {
    fn is_fresh(&self) -> bool {
        self.fetched_at.elapsed() < TTL
    }
}

pub struct AppState {
    pub fetcher: Arc<HttpFetcher>,
    pub cfg: Config,
    /// 라이브러리. 열지 못했으면 `None`이고, 그래도 검색은 동작한다.
    pub store: Option<tsuburu_store::Store>,
    /// Dialogue indexing. `None` where the platform has no OCR.
    pub grinder: Option<Arc<crate::grinder::Grinder>>,
    /// Where exported dialogue shards are written.
    pub shards_dir: Option<std::path::PathBuf>,
    version: RwLock<Option<Cached<String>>>,
    gg: RwLock<Option<Cached<GgMap>>>,
    /// 갤러리 메타 JSON은 최대 200 KB를 넘기도 한다. 카드 한 장으로 줄여
    /// 캐시해두면 같은 결과를 다시 그릴 때 네트워크를 타지 않는다.
    pub cards: Cache<i32, Card>,
}

impl AppState {
    pub fn new(fetcher: Arc<HttpFetcher>, cfg: Config) -> Self {
        Self::with_store(fetcher, cfg, None)
    }

    pub fn with_store(
        fetcher: Arc<HttpFetcher>,
        cfg: Config,
        store: Option<tsuburu_store::Store>,
    ) -> Self {
        Self::full(fetcher, cfg, store, None)
    }

    pub fn full(
        fetcher: Arc<HttpFetcher>,
        cfg: Config,
        store: Option<tsuburu_store::Store>,
        grinder: Option<Arc<crate::grinder::Grinder>>,
    ) -> Self {
        Self {
            fetcher,
            cfg,
            store,
            grinder,
            shards_dir: None,
            version: RwLock::new(None),
            gg: RwLock::new(None),
            cards: Cache::new(4096),
        }
    }

    pub fn with_shards_dir(mut self, dir: std::path::PathBuf) -> Self {
        self.shards_dir = Some(dir);
        self
    }

    pub async fn version(&self) -> Result<String, tsuburu_hitomi::SearchError> {
        if let Some(cached) = self.version.read().await.as_ref()
            && cached.is_fresh()
        {
            return Ok(cached.value.clone());
        }
        let fresh =
            tsuburu_hitomi::galleries_index_version(self.fetcher.as_ref(), &self.cfg).await?;
        *self.version.write().await =
            Some(Cached { value: fresh.clone(), fetched_at: Instant::now() });
        Ok(fresh)
    }

    /// 인덱스 상위 노드를 미리 받아 캐시에 얹는다.
    ///
    /// 검색은 노드를 직렬로 타고 내려가므로 왕복 지연이 그대로 누적된다.
    /// 상위 노드는 모든 검색이 공유하니, 서버가 뜰 때 배경에서 받아두면
    /// 첫 검색부터 짧아진다. 실패해도 검색은 그대로 동작한다.
    pub async fn warm(&self, levels: usize) {
        let Ok(version) = self.version().await else {
            tracing::debug!("skipping index warm-up: version unavailable");
            return;
        };
        let url = self.cfg.galleries_index_url(&version);
        match tsuburu_hitomi::warm_index(self.fetcher.as_ref(), &url, levels).await {
            Ok(nodes) => tracing::info!(nodes, "warmed the index"),
            Err(err) => tracing::debug!(%err, "index warm-up failed"),
        }
    }

    pub async fn gg(&self) -> Result<GgMap, tsuburu_hitomi::GalleryFetchError> {
        if let Some(cached) = self.gg.read().await.as_ref()
            && cached.is_fresh()
        {
            return Ok(cached.value.clone());
        }
        self.refresh_gg().await
    }

    /// `gg.js`를 다시 받아 캐시를 갈아끼운다.
    ///
    /// 이미지 경로 접두사(`gg.b`)는 주기적으로 바뀐다. 낡은 값으로 만든 URL은
    /// 404가 나므로, 이미지 프록시가 404를 만나면 이것을 호출해 한 번 다시
    /// 시도한다. TTL만 믿으면 회전과 만료 사이의 구간에서 이미지가 전부
    /// 깨진 것처럼 보인다.
    pub async fn refresh_gg(&self) -> Result<GgMap, tsuburu_hitomi::GalleryFetchError> {
        let fresh = tsuburu_hitomi::fetch_gg(self.fetcher.as_ref(), &self.cfg).await?;
        *self.gg.write().await =
            Some(Cached { value: fresh.clone(), fetched_at: Instant::now() });
        Ok(fresh)
    }
}
