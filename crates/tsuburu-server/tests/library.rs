//! 즐겨찾기와 읽음 기록 API. 네트워크를 타지 않는다.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use std::sync::Arc;
use tower::ServiceExt;
use tsuburu_fetch::{FetchConfig, HttpFetcher};
use tsuburu_hitomi::Config;
use tsuburu_server::{AppState, router};

fn app_with_library(dir: &tempfile::TempDir) -> axum::Router {
    let store = tsuburu_store::Store::open(dir.path().join("test.redb")).unwrap();
    let fetcher = Arc::new(HttpFetcher::new(FetchConfig::default()).unwrap());
    router(Arc::new(AppState::with_store(fetcher, Config::default(), Some(store))))
}

fn app_without_library() -> axum::Router {
    let fetcher = Arc::new(HttpFetcher::new(FetchConfig::default()).unwrap());
    router(Arc::new(AppState::new(fetcher, Config::default())))
}

async fn call(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    let request = match body {
        Some(json) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json.to_string()))
            .unwrap(),
        None => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            builder.body(Body::empty()).unwrap()
        }
    };
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
}

fn summary() -> serde_json::Value {
    serde_json::json!({
        "title": "Test Gallery",
        "language": "japanese",
        "kind": "doujinshi",
        "pages": 20,
        "thumbnail_hash": "a".repeat(64),
    })
}

#[tokio::test]
async fn favorites_can_be_added_listed_and_removed() {
    let dir = tempfile::tempdir().unwrap();

    let (status, body) = call(app_with_library(&dir), "PUT", "/api/favorites/42", Some(summary())).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], 42);
    assert_eq!(body["title"], "Test Gallery");

    let (status, body) = call(app_with_library(&dir), "GET", "/api/favorites", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["items"][0]["pages"], 20);

    let (status, body) = call(app_with_library(&dir), "DELETE", "/api/favorites/42", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["removed"], true);

    let (_, body) = call(app_with_library(&dir), "GET", "/api/favorites", None).await;
    assert!(body["items"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn progress_is_stored_and_returned() {
    let dir = tempfile::tempdir().unwrap();
    let mut body = summary();
    body["page"] = serde_json::json!(7);

    let (status, stored) = call(app_with_library(&dir), "PUT", "/api/history/9", Some(body)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(stored["last_page"], 7);

    let (_, listed) = call(app_with_library(&dir), "GET", "/api/history", None).await;
    assert_eq!(listed["items"][0]["id"], 9);
    assert_eq!(listed["items"][0]["last_page"], 7);
}

#[tokio::test]
async fn clearing_history_keeps_favorites() {
    let dir = tempfile::tempdir().unwrap();
    let mut progress = summary();
    progress["page"] = serde_json::json!(1);

    call(app_with_library(&dir), "PUT", "/api/favorites/5", Some(summary())).await;
    call(app_with_library(&dir), "PUT", "/api/history/5", Some(progress)).await;

    let (status, _) = call(app_with_library(&dir), "DELETE", "/api/history", None).await;
    assert_eq!(status, StatusCode::OK);

    let (_, history) = call(app_with_library(&dir), "GET", "/api/history", None).await;
    assert!(history["items"].as_array().unwrap().is_empty());

    let (_, favorites) = call(app_with_library(&dir), "GET", "/api/favorites", None).await;
    assert_eq!(favorites["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn library_endpoints_report_a_storage_error_when_unavailable() {
    let (status, body) = call(app_without_library(), "GET", "/api/favorites", None).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["error"], "storage");
    assert!(
        body["message"].as_str().unwrap().contains("favorites and history are disabled"),
        "the message must say what stopped working: {body}"
    );
}
