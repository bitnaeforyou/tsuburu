//! Getting a written file off the machine: what is refused, and what is said.
//!
//! Nothing here lets a request reach the platform itself. Handing a file over
//! on a computer opens the file manager, and a test suite that opens windows
//! on the machine running it is not one anybody can run while working - so
//! every case stops at a refusal, and existence is checked before the
//! platform is asked, which is what makes that possible.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use std::sync::Arc;
use tower::ServiceExt;
use tsuburu_fetch::{FetchConfig, HttpFetcher};
use tsuburu_hitomi::Config;
use tsuburu_server::guard::Reach;
use tsuburu_server::{AppState, router};

fn app(shards: Option<std::path::PathBuf>) -> axum::Router {
    let fetcher = Arc::new(HttpFetcher::new(FetchConfig::default()).unwrap());
    let cfg =
        Config { scheme: "http".into(), ltn_domain: "127.0.0.1:9".into(), ..Config::default() };
    let mut state = AppState::full(fetcher, cfg, None, None);
    if let Some(dir) = shards {
        state = state.with_shards_dir(dir);
    }
    router(Arc::new(state), Reach::ThisMachine)
}

async fn call(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: Body,
) -> (StatusCode, serde_json::Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

/// The interface asks this before it offers anything, so it has to answer on
/// every platform rather than only where there is something to do.
#[tokio::test]
async fn the_machine_says_what_it_does_with_a_written_file() {
    let (status, body) = call(app(None), "GET", "/api/handoff", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    let said = body["handoff"].as_str().unwrap();
    assert!(matches!(said, "share" | "reveal" | "none"), "{said}");
    // A computer shows a file where it is; only a phone has to send it.
    assert_eq!(
        said,
        if cfg!(any(target_os = "android", target_os = "ios")) { "share" } else { "reveal" }
    );
}

#[tokio::test]
async fn a_name_that_is_a_path_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["../escaped.json", "sub/escaped.json", "..", ""] {
        let uri = format!("/api/handoff?name={}", urlencoding(name));
        let (status, _) = call(app(Some(dir.path().into())), "POST", &uri, Body::from("{}")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{name} was taken");
    }
    // Nothing was written on the way to any of those refusals.
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn an_empty_body_is_refused_rather_than_written() {
    let dir = tempfile::tempdir().unwrap();
    let (status, body) = call(
        app(Some(dir.path().into())),
        "POST",
        "/api/handoff?name=tsuburu-backup.json",
        Body::empty(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["message"].as_str().unwrap().contains("nothing"));
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn only_a_shard_name_names_a_shard() {
    let dir = tempfile::tempdir().unwrap();
    let body = serde_json::json!({ "names": ["../library.redb"] }).to_string();
    let request = Request::builder()
        .method("POST")
        .uri("/api/dialogue/shards/hand-over")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap();
    let response = app(Some(dir.path().into())).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// A well-formed name for a file that is not there stops before the platform
/// is asked, which is why handing over can be tested at all.
#[tokio::test]
async fn a_shard_that_is_not_there_is_said_to_be_missing() {
    let dir = tempfile::tempdir().unwrap();
    let body = serde_json::json!({ "names": ["dialogue-1-2-0123456789abcdef.tsd"] }).to_string();
    let request = Request::builder()
        .method("POST")
        .uri("/api/dialogue/shards/hand-over")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap();
    let response = app(Some(dir.path().into())).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["code"], "no_handoff");
    assert!(json["message"].as_str().unwrap().contains("not there"));
}

#[tokio::test]
async fn shards_already_written_are_listed_without_an_export() {
    let dir = tempfile::tempdir().unwrap();
    let shard = "dialogue-100-199-fedcba9876543210.tsd";
    std::fs::write(dir.path().join(shard), b"not really a shard").unwrap();
    // Everything else in the directory is somebody else's business.
    std::fs::write(dir.path().join("notes.txt"), b"x").unwrap();

    let (status, body) =
        call(app(Some(dir.path().into())), "GET", "/api/dialogue/shards", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    let files = body["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["name"], shard);
    assert_eq!(files[0]["bytes"], 18);
}

fn urlencoding(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}
