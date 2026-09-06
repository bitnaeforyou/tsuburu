//! `tsuburu-hitomi`가 네트워크를 하지 않게 만드는 경계.
//!
//! 이 crate의 모든 파싱과 탐색은 이 트레잇 위에서 돌기 때문에, 저장된 응답
//! 바이트만으로 오프라인 테스트가 가능하다. 문서화되지 않은 바이너리 포맷을
//! 다루는 이상 이것이 유일하게 믿을 수 있는 검증 수단이다.

use std::future::Future;
use std::ops::Range;
use std::pin::Pin;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("network error: {0}")]
    Network(String),
    #[error("unexpected status {0}")]
    Status(u16),
}

/// 트레잇 객체로 쓸 수 있도록 future를 박싱한다. 이 crate의 작업은 전부
/// 네트워크 대기가 지배적이라 할당 비용은 측정 가능한 수준이 아니다.
pub trait Fetcher: Send + Sync {
    fn get_range<'a>(
        &'a self,
        url: &'a str,
        range: Range<u64>,
    ) -> BoxFuture<'a, Result<Vec<u8>, FetchError>>;

    fn get<'a>(&'a self, url: &'a str) -> BoxFuture<'a, Result<Vec<u8>, FetchError>>;

    /// 본문을 받지 않고 길이만 묻는다.
    ///
    /// `.nozomi` 목록은 최대 4.8 MB지만 전체 개수를 알아야 페이지를 매길 수
    /// 있다. 길이를 4로 나누면 갤러리 수다.
    fn length<'a>(&'a self, url: &'a str) -> BoxFuture<'a, Result<u64, FetchError>>;
}

#[cfg(any(test, feature = "mock"))]
pub mod mock {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct MockFetcher {
        pub bodies: HashMap<String, Vec<u8>>,
        pub calls: Mutex<Vec<(String, Range<u64>)>>,
    }

    impl MockFetcher {
        pub fn with(url: &str, body: Vec<u8>) -> Self {
            let mut bodies = HashMap::new();
            bodies.insert(url.to_string(), body);
            Self { bodies, calls: Mutex::new(Vec::new()) }
        }

        pub fn insert(mut self, url: &str, body: Vec<u8>) -> Self {
            self.bodies.insert(url.to_string(), body);
            self
        }

        pub fn call_count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }
    }

    impl Fetcher for MockFetcher {
        fn get_range<'a>(
            &'a self,
            url: &'a str,
            range: Range<u64>,
        ) -> BoxFuture<'a, Result<Vec<u8>, FetchError>> {
            Box::pin(async move {
                self.calls.lock().unwrap().push((url.to_string(), range.clone()));
                let body = self.bodies.get(url).ok_or(FetchError::Status(404))?;
                let start = (range.start as usize).min(body.len());
                let end = (range.end as usize).min(body.len());
                Ok(body[start..end].to_vec())
            })
        }

        fn get<'a>(&'a self, url: &'a str) -> BoxFuture<'a, Result<Vec<u8>, FetchError>> {
            Box::pin(async move {
                self.bodies.get(url).cloned().ok_or(FetchError::Status(404))
            })
        }

        fn length<'a>(&'a self, url: &'a str) -> BoxFuture<'a, Result<u64, FetchError>> {
            Box::pin(async move {
                self.bodies
                    .get(url)
                    .map(|b| b.len() as u64)
                    .ok_or(FetchError::Status(404))
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_returns_requested_range() {
        let f = mock::MockFetcher::with("u", (0u8..10).collect());
        assert_eq!(f.get_range("u", 2..5).await.unwrap(), vec![2, 3, 4]);
    }

    #[tokio::test]
    async fn mock_clamps_range_past_end() {
        let f = mock::MockFetcher::with("u", vec![1, 2, 3]);
        assert_eq!(f.get_range("u", 1..99).await.unwrap(), vec![2, 3]);
    }

    #[tokio::test]
    async fn mock_reports_missing_url() {
        let f = mock::MockFetcher::default();
        assert!(matches!(f.get("nope").await, Err(FetchError::Status(404))));
    }
}
