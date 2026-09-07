//! Dialogue pipeline and API with a mock OCR. No network, no Vision.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use std::sync::Arc;
use tower::ServiceExt;
use tsuburu_dialogue::{DialogueStore, Priority};
use tsuburu_fetch::{FetchConfig, HttpFetcher};
use tsuburu_hitomi::Config;
use tsuburu_ocr::MockOcr;
use tsuburu_server::grinder::Grinder;
use tsuburu_server::{AppState, router};

fn grinder(dir: &tempfile::TempDir, ocr: MockOcr) -> Arc<Grinder> {
    let store = DialogueStore::open(dir.path().join("dialogue.redb")).unwrap();
    let fetcher = Arc::new(HttpFetcher::new(FetchConfig::default()).unwrap());
    // Point at a dead address: nothing here should need the network.
    let cfg = Config { scheme: "http".into(), ltn_domain: "127.0.0.1:9".into(), ..Config::default() };
    Arc::new(Grinder::new(fetcher, cfg, Arc::new(store), Arc::new(ocr)))
}

fn app(grinder: Option<Arc<Grinder>>) -> axum::Router {
    let fetcher = Arc::new(HttpFetcher::new(FetchConfig::default()).unwrap());
    let cfg = Config { scheme: "http".into(), ltn_domain: "127.0.0.1:9".into(), ..Config::default() };
    router(Arc::new(AppState::full(fetcher, cfg, None, grinder)))
}

async fn call(app: axum::Router, method: &str, uri: &str, body: Option<serde_json::Value>) -> (StatusCode, serde_json::Value) {
    let builder = Request::builder().method(method).uri(uri).header(header::CONTENT_TYPE, "application/json");
    let request = match body {
        Some(json) => builder.body(Body::from(json.to_string())).unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
}

#[tokio::test]
async fn recognised_pages_are_stored_in_reading_order() {
    let dir = tempfile::tempdir().unwrap();
    let g = grinder(&dir, MockOcr::saying(&["일단", "구급차라도", "부르는 게 좋겠어요"]));

    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    tx.send((1u16, b"page-one".to_vec())).await.unwrap();
    tx.send((0u16, b"page-zero".to_vec())).await.unwrap();
    drop(tx);

    let pages = g.recognise(&mut rx).await.unwrap();
    assert_eq!(pages.iter().map(|p| p.page).collect::<Vec<_>>(), vec![0, 1]);
    assert_eq!(pages[0].lines, vec!["일단", "구급차라도", "부르는 게 좋겠어요"]);

    g.store().enqueue(&[42], Priority::Background).unwrap();
    g.store().complete(42, &pages).unwrap();
    let hits = g.store().search("구급차라도 부르는 게", 10).unwrap();
    assert_eq!(hits[0].gallery_id, 42);
    assert!(hits[0].exact);
}

#[tokio::test]
async fn a_gallery_whose_pages_all_fail_is_an_error_not_an_empty_success() {
    let dir = tempfile::tempdir().unwrap();
    let g = grinder(&dir, MockOcr { lines: vec![], fail: true });
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    tx.send((0u16, b"x".to_vec())).await.unwrap();
    drop(tx);
    assert!(g.recognise(&mut rx).await.is_err());
}

#[tokio::test]
async fn status_reports_unsupported_without_an_ocr() {
    let (status, body) = call(app(None), "GET", "/api/dialogue/status", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["supported"], false);

    let (status, body) = call(app(None), "GET", "/api/dialogue/search?q=x", None).await;
    assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    assert_eq!(body["error"], "unsupported");
}

#[tokio::test]
async fn pasted_history_is_queued_ahead_of_everything() {
    let dir = tempfile::tempdir().unwrap();
    let g = grinder(&dir, MockOcr::default());
    g.store().enqueue(&[1], Priority::Background).unwrap();

    let body = serde_json::json!({
        "text": "https://hitomi.la/doujinshi/x-korean-777.html\n888",
    });
    let (status, response) = call(app(Some(Arc::clone(&g))), "POST", "/api/dialogue/enqueue", Some(body)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["found"], 2);
    assert_eq!(response["added"], 2);
    assert_eq!(g.store().next_pending().unwrap(), Some(777));
}

#[tokio::test]
async fn settings_round_trip_and_default_off() {
    let dir = tempfile::tempdir().unwrap();
    {
        let g = grinder(&dir, MockOcr::default());
        assert!(!g.settings().await.enabled);

        let body = serde_json::json!({
            "enabled": true, "language": "korean", "kinds": ["doujinshi"], "bytes_per_second": 1048576
        });
        let (status, _) = call(app(Some(Arc::clone(&g))), "PUT", "/api/dialogue/settings", Some(body)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(g.settings().await.enabled);
        assert_eq!(g.settings().await.kinds, vec!["doujinshi"]);
        // redb allows one open handle per file; release it before reopening.
    }

    // Persisted: a fresh grinder over the same store starts enabled.
    let store = DialogueStore::open(dir.path().join("dialogue.redb")).unwrap();
    let fetcher = Arc::new(HttpFetcher::new(FetchConfig::default()).unwrap());
    let again = Grinder::new(fetcher, Config::default(), Arc::new(store), Arc::new(MockOcr::default()));
    assert!(again.settings().await.enabled);
}

#[tokio::test]
async fn search_api_returns_hits_with_counts() {
    let dir = tempfile::tempdir().unwrap();
    let g = grinder(&dir, MockOcr::default());
    g.store().enqueue(&[5], Priority::Background).unwrap();
    g.store()
        .complete(5, &[tsuburu_dialogue::PageText { page: 3, lines: vec!["뭐라고".into(), "안녕하세요".into()] }])
        .unwrap();

    let (status, body) = call(app(Some(g)), "GET", "/api/dialogue/search?q=%EC%95%88%EB%85%95%ED%95%98%EC%84%B8%EC%9A%94", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["hits"][0]["gallery_id"], 5);
    assert_eq!(body["hits"][0]["page"], 3);
    assert_eq!(body["counts"]["done"], 1);
}
