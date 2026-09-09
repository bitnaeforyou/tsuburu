//! [`Fetcher`]의 실제 구현.
//!
//! 스펙 12절이 지목한 최적화가 여기 들어간다. 실측 결과 검색 비용의 대부분은
//! B-tree 하강에 쓰이는 직렬 왕복(깊이 6~7 × 약 200 ms)이고, 각 요청은 464
//! 바이트에 불과했다. 상위 노드는 모든 검색에서 동일하므로 캐시 적중률이
//! 높고, 캐싱만으로 왕복이 크게 준다.

use quick_cache::sync::Cache;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::Semaphore;
use tsuburu_hitomi::fetcher::BoxFuture;
use tsuburu_hitomi::{FetchError, Fetcher};

#[derive(Debug, Clone)]
pub struct FetchConfig {
    /// hitomi에 동시에 열어두는 요청 수. 상대 서버를 배려하는 상한이다.
    pub max_concurrent: usize,
    /// 캐시에 둘 range 응답 개수. 노드 하나가 464바이트이므로 4096개는 2 MB 남짓이다.
    pub cache_entries: usize,
    pub user_agent: String,
    pub referer: String,
    pub timeout: Duration,
    /// Attempts per request, including the first.
    ///
    /// A long-lived client can keep a pooled connection that has gone bad,
    /// and every request routed onto it fails the same way. Retrying moves
    /// the request to another connection.
    pub attempts: usize,
}

impl Default for FetchConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 8,
            cache_entries: 4096,
            user_agent: concat!("tsuburu/", env!("CARGO_PKG_VERSION")).into(),
            referer: "https://hitomi.la/".into(),
            timeout: Duration::from_secs(20),
            attempts: 3,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub requests: u64,
    pub cache_hits: u64,
    pub bytes: u64,
}

#[derive(Default)]
struct Counters {
    requests: AtomicU64,
    cache_hits: AtomicU64,
    bytes: AtomicU64,
}

pub struct HttpFetcher {
    client: reqwest::Client,
    cache: Cache<(String, u64, u64), Arc<Vec<u8>>>,
    /// `.nozomi` 목록의 길이. 목록마다 한 번만 물으면 된다.
    lengths: Cache<String, u64>,
    limiter: Semaphore,
    counters: Counters,
    referer: String,
    attempts: usize,
}

impl HttpFetcher {
    pub fn new(cfg: FetchConfig) -> Result<Self, FetchError> {
        let client = reqwest::Client::builder()
            .user_agent(cfg.user_agent.clone())
            .timeout(cfg.timeout)
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
            .map_err(|e| FetchError::Network(e.to_string()))?;

        Ok(Self {
            client,
            cache: Cache::new(cfg.cache_entries),
            lengths: Cache::new(256),
            limiter: Semaphore::new(cfg.max_concurrent),
            counters: Counters::default(),
            referer: cfg.referer,
            attempts: cfg.attempts.max(1),
        })
    }

    pub fn stats(&self) -> Stats {
        Stats {
            requests: self.counters.requests.load(Ordering::Relaxed),
            cache_hits: self.counters.cache_hits.load(Ordering::Relaxed),
            bytes: self.counters.bytes.load(Ordering::Relaxed),
        }
    }

    /// Posts JSON and returns the body. Used for the embedding endpoint,
    /// which is the user's own server rather than hitomi.
    pub async fn post_json(&self, url: &str, body: &str) -> Result<String, FetchError> {
        let response = self
            .client
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| FetchError::Network(e.to_string()))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(FetchError::Status(status));
        }
        response.text().await.map_err(|e| FetchError::Network(e.to_string()))
    }

    /// 이미지처럼 캐시하지 않고 그대로 흘려보낼 응답. 서버의 프록시가 쓴다.
    pub async fn stream(&self, url: &str) -> Result<reqwest::Response, FetchError> {
        let response = self
            .client
            .get(url)
            .header(reqwest::header::REFERER, &self.referer)
            .send()
            .await
            .map_err(|e| FetchError::Network(e.to_string()))?;
        check_status(response.status().as_u16())?;
        self.counters.requests.fetch_add(1, Ordering::Relaxed);
        Ok(response)
    }

    /// Retries transient failures. A refusal from the server is final; a
    /// broken connection is not.
    async fn send(&self, url: &str, range: Option<Range<u64>>) -> Result<Vec<u8>, FetchError> {
        let _permit =
            self.limiter.acquire().await.map_err(|e| FetchError::Network(e.to_string()))?;

        let mut last = None;
        for attempt in 0..self.attempts {
            match self.attempt(url, range.clone()).await {
                Ok(body) => return Ok(body),
                Err(err) if !is_retryable(&err) => return Err(err),
                Err(err) => {
                    tracing::debug!(%err, attempt, url, "retrying");
                    last = Some(err);
                    tokio::time::sleep(backoff(attempt)).await;
                }
            }
        }
        Err(last.unwrap_or_else(|| FetchError::Network("no attempts were made".into())))
    }

    async fn attempt(&self, url: &str, range: Option<Range<u64>>) -> Result<Vec<u8>, FetchError> {
        let mut request = self.client.get(url).header(reqwest::header::REFERER, &self.referer);
        if let Some(r) = &range {
            // Range 헤더의 끝은 포함이다. 호출 쪽은 반열린 구간을 쓴다.
            request = request.header(
                reqwest::header::RANGE,
                format!("bytes={}-{}", r.start, r.end.saturating_sub(1)),
            );
        }

        let response = request.send().await.map_err(|e| FetchError::Network(e.to_string()))?;
        check_status(response.status().as_u16())?;

        let bytes = response.bytes().await.map_err(|e| FetchError::Network(e.to_string()))?;

        self.counters.requests.fetch_add(1, Ordering::Relaxed);
        self.counters.bytes.fetch_add(bytes.len() as u64, Ordering::Relaxed);
        Ok(bytes.to_vec())
    }
}

/// Server refusals are final; transport failures are worth another try.
fn is_retryable(err: &FetchError) -> bool {
    match err {
        FetchError::Network(_) => true,
        FetchError::Status(code) => (500..600).contains(code),
    }
}

fn backoff(attempt: usize) -> Duration {
    Duration::from_millis(100 << attempt.min(4))
}

/// Extracts `12345` from `bytes 0-0/12345`. An unknown total (`*`) is ignored.
fn total_from_content_range(value: &str) -> Option<u64> {
    value.rsplit_once('/').and_then(|(_, total)| total.trim().parse().ok())
}

/// 206은 Range 응답, 200은 서버가 Range를 무시하고 전체를 준 경우다.
fn check_status(status: u16) -> Result<(), FetchError> {
    if status == 200 || status == 206 { Ok(()) } else { Err(FetchError::Status(status)) }
}

impl Fetcher for HttpFetcher {
    fn get_range<'a>(
        &'a self,
        url: &'a str,
        range: Range<u64>,
    ) -> BoxFuture<'a, Result<Vec<u8>, FetchError>> {
        Box::pin(async move {
            let key = (url.to_string(), range.start, range.end);
            if let Some(hit) = self.cache.get(&key) {
                self.counters.cache_hits.fetch_add(1, Ordering::Relaxed);
                return Ok(hit.as_ref().clone());
            }

            let body = self.send(url, Some(range)).await?;
            self.cache.insert(key, Arc::new(body.clone()));
            Ok(body)
        })
    }

    fn get<'a>(&'a self, url: &'a str) -> BoxFuture<'a, Result<Vec<u8>, FetchError>> {
        Box::pin(async move { self.send(url, None).await })
    }

    fn length<'a>(&'a self, url: &'a str) -> BoxFuture<'a, Result<u64, FetchError>> {
        Box::pin(async move {
            if let Some(cached) = self.lengths.get(&url.to_string()) {
                return Ok(cached);
            }

            // HEAD 대신 첫 1바이트를 Range로 요청한다. 206 응답의
            // `Content-Range: bytes 0-0/12345` 마지막 숫자가 전체 길이다.
            // Range를 무시하고 200으로 전부 주는 서버에서는 Content-Length가
            // 곧 전체 길이이므로 양쪽 모두 처리된다.
            let _permit =
                self.limiter.acquire().await.map_err(|e| FetchError::Network(e.to_string()))?;
            let response = self
                .client
                .get(url)
                .header(reqwest::header::REFERER, &self.referer)
                .header(reqwest::header::RANGE, "bytes=0-0")
                .send()
                .await
                .map_err(|e| FetchError::Network(e.to_string()))?;
            check_status(response.status().as_u16())?;
            self.counters.requests.fetch_add(1, Ordering::Relaxed);

            let length = response
                .headers()
                .get(reqwest::header::CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .and_then(total_from_content_range)
                .or_else(|| response.content_length())
                .ok_or_else(|| FetchError::Network("server did not report a length".into()))?;

            self.lengths.insert(url.to_string(), length);
            Ok(length)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn repeated_node_request_hits_cache() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/idx"))
            .respond_with(ResponseTemplate::new(206).set_body_bytes(vec![7u8; 464]))
            .expect(1)
            .mount(&server)
            .await;

        let fetcher = HttpFetcher::new(FetchConfig::default()).unwrap();
        let url = format!("{}/idx", server.uri());

        let first = fetcher.get_range(&url, 0..464).await.unwrap();
        let second = fetcher.get_range(&url, 0..464).await.unwrap();
        assert_eq!(first, second);

        let stats = fetcher.stats();
        assert_eq!(stats.requests, 1, "second call must come from cache");
        assert_eq!(stats.cache_hits, 1);
        // expect(1)이 drop 시점에 검증된다
    }

    #[tokio::test]
    async fn different_ranges_are_cached_separately() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/idx"))
            .respond_with(ResponseTemplate::new(206).set_body_bytes(vec![1u8; 8]))
            .expect(2)
            .mount(&server)
            .await;

        let fetcher = HttpFetcher::new(FetchConfig::default()).unwrap();
        let url = format!("{}/idx", server.uri());
        fetcher.get_range(&url, 0..8).await.unwrap();
        fetcher.get_range(&url, 8..16).await.unwrap();
        assert_eq!(fetcher.stats().requests, 2);
    }

    #[tokio::test]
    async fn range_header_uses_inclusive_end() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/idx"))
            .and(wiremock::matchers::header("range", "bytes=0-463"))
            .respond_with(ResponseTemplate::new(206).set_body_bytes(vec![0u8; 464]))
            .expect(1)
            .mount(&server)
            .await;

        let fetcher = HttpFetcher::new(FetchConfig::default()).unwrap();
        fetcher.get_range(&format!("{}/idx", server.uri()), 0..464).await.unwrap();
    }

    #[test]
    fn parses_the_total_from_content_range() {
        assert_eq!(total_from_content_range("bytes 0-0/397352"), Some(397352));
        assert_eq!(total_from_content_range("bytes 0-0/*"), None);
        assert_eq!(total_from_content_range("nonsense"), None);
    }

    #[tokio::test]
    async fn length_reads_the_total_from_a_range_response() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/list"))
            .respond_with(
                ResponseTemplate::new(206)
                    .insert_header("content-range", "bytes 0-0/4815272")
                    .set_body_bytes(vec![0u8]),
            )
            .mount(&server)
            .await;

        let fetcher = HttpFetcher::new(FetchConfig::default()).unwrap();
        let url = format!("{}/list", server.uri());
        assert_eq!(fetcher.length(&url).await.unwrap(), 4815272);
        // 두 번째 호출은 캐시에서 온다
        assert_eq!(fetcher.length(&url).await.unwrap(), 4815272);
        assert_eq!(fetcher.stats().requests, 1);
    }

    #[tokio::test]
    async fn length_falls_back_to_content_length_when_range_is_ignored() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/list"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![7u8; 40]))
            .mount(&server)
            .await;

        let fetcher = HttpFetcher::new(FetchConfig::default()).unwrap();
        assert_eq!(fetcher.length(&format!("{}/list", server.uri())).await.unwrap(), 40);
    }

    #[test]
    fn only_transport_failures_are_retried() {
        assert!(is_retryable(&FetchError::Network("reset".into())));
        assert!(is_retryable(&FetchError::Status(503)));
        assert!(!is_retryable(&FetchError::Status(404)));
        assert!(!is_retryable(&FetchError::Status(206)));
    }

    #[tokio::test]
    async fn a_failed_attempt_is_retried() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/flaky"))
            .respond_with(ResponseTemplate::new(500))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/flaky"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![9u8; 4]))
            .expect(1)
            .mount(&server)
            .await;

        let fetcher = HttpFetcher::new(FetchConfig::default()).unwrap();
        let body = fetcher.get(&format!("{}/flaky", server.uri())).await.unwrap();
        assert_eq!(body, vec![9u8; 4]);
    }

    #[tokio::test]
    async fn a_refusal_is_not_retried() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/gone"))
            .respond_with(ResponseTemplate::new(404))
            .expect(1)
            .mount(&server)
            .await;

        let fetcher = HttpFetcher::new(FetchConfig::default()).unwrap();
        let err = fetcher.get(&format!("{}/gone", server.uri())).await.unwrap_err();
        assert!(matches!(err, FetchError::Status(404)));
        // expect(1) fails on drop if we retried a refusal.
    }

    #[tokio::test]
    async fn error_status_is_reported() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/missing"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let fetcher = HttpFetcher::new(FetchConfig::default()).unwrap();
        let err = fetcher.get(&format!("{}/missing", server.uri())).await.unwrap_err();
        assert!(matches!(err, FetchError::Status(404)));
    }
}
