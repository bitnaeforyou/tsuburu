//! API 계약 테스트.
//!
//! 목 서버를 hitomi 자리에 세우고 라우터를 그대로 호출한다. 네트워크를 타지
//! 않으므로 기본 `cargo test`에서 돈다.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use std::sync::Arc;
use tower::ServiceExt;
use tsuburu_fetch::{FetchConfig, HttpFetcher};
use tsuburu_hitomi::Config;
use tsuburu_server::{AppState, router};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const HASH: &str = "637a35d9d5a892a8b86b97fe9b42e5cf49b8edd6685f7e1baf8f3b361f6cd3e2";
const VERSION: &str = "1788594832";

/// 검색어 하나만 담은 리프 노드. 포맷은 스펙 부록 A를 따른다.
fn leaf_index(key: [u8; 4], entry: (u64, i32)) -> Vec<u8> {
    let mut node = Vec::new();
    node.extend_from_slice(&1i32.to_be_bytes()); // number_of_keys
    node.extend_from_slice(&4i32.to_be_bytes()); // key_size
    node.extend_from_slice(&key);
    node.extend_from_slice(&1i32.to_be_bytes()); // number_of_datas
    node.extend_from_slice(&entry.0.to_be_bytes());
    node.extend_from_slice(&entry.1.to_be_bytes());
    node.extend_from_slice(&[0u8; 8 * 17]); // subnode addresses, all zero -> leaf
    node.resize(464, 0);
    node
}

fn id_block(ids: &[i32]) -> Vec<u8> {
    let mut out = (ids.len() as i32).to_be_bytes().to_vec();
    for id in ids {
        out.extend_from_slice(&id.to_be_bytes());
    }
    out
}

fn gg_js() -> String {
    "gg = { m: function(g) { var o = 1; switch (g) { case 574: o = 0; break; } return o; }, \
     b: '999/' };"
        .into()
}

fn gallery_js() -> String {
    format!(
        r#"var galleryinfo = {{"title":"Test Gallery","type":"doujinshi","language":"japanese",
        "galleryurl":"/doujinshi/test-1.html","date":"2026-01-01",
        "tags":[{{"tag":"glasses","female":"","male":""}}],
        "files":[{{"hash":"{HASH}","name":"001.jpg","width":100,"height":200,"hasavif":1}}]}};"#
    )
}

async fn hitomi_stub() -> MockServer {
    let server = MockServer::start().await;
    let ids = id_block(&[300, 200, 100]);

    Mock::given(method("GET"))
        .and(path("/galleriesindex/version"))
        .respond_with(ResponseTemplate::new(200).set_body_string(VERSION))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/galleriesindex/galleries.{VERSION}.index")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(leaf_index(tsuburu_hitomi::hash_term("naruto"), (0, 16))),
        )
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/galleriesindex/galleries.{VERSION}.data")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(ids))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/gg.js"))
        .respond_with(ResponseTemplate::new(200).set_body_string(gg_js()))
        .mount(&server)
        .await;

    for id in [100, 200, 300] {
        Mock::given(method("GET"))
            .and(path(format!("/galleries/{id}.js")))
            .respond_with(ResponseTemplate::new(200).set_body_string(gallery_js()))
            .mount(&server)
            .await;
    }

    server
}

fn app(server: &MockServer) -> axum::Router {
    let host = server.uri().trim_start_matches("http://").to_string();
    let cfg = Config { scheme: "http".into(), ltn_domain: host, ..Config::default() };
    let fetcher = Arc::new(HttpFetcher::new(FetchConfig::default()).unwrap());
    router(Arc::new(AppState::new(fetcher, cfg)))
}

async fn get_json(app: axum::Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
async fn search_returns_ids_and_total() {
    let server = hitomi_stub().await;
    let (status, body) = get_json(app(&server), "/api/search?q=naruto").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 3);
    assert_eq!(body["ids"], serde_json::json!([300, 200, 100]));
}

#[tokio::test]
async fn search_honours_offset_and_limit() {
    let server = hitomi_stub().await;
    let (status, body) = get_json(app(&server), "/api/search?q=naruto&offset=1&limit=1").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 3, "total is the whole result set, not the page");
    assert_eq!(body["ids"], serde_json::json!([200]));
}

#[tokio::test]
async fn search_without_terms_is_a_bad_request() {
    let server = hitomi_stub().await;
    let (status, body) = get_json(app(&server), "/api/search?q=").await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "bad_request");
}

#[tokio::test]
async fn unknown_term_returns_an_empty_page() {
    let server = hitomi_stub().await;
    let (status, body) = get_json(app(&server), "/api/search?q=nothingmatchesthis").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 0);
    assert_eq!(body["ids"], serde_json::json!([]));
}

#[tokio::test]
async fn cards_summarise_galleries_without_exposing_hitomi() {
    let server = hitomi_stub().await;
    let (status, body) = get_json(app(&server), "/api/cards?ids=300,200").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 2);
    assert_eq!(body[0]["title"], "Test Gallery");
    assert_eq!(body[0]["pages"], 1);
    assert_eq!(body[0]["tags"], serde_json::json!(["glasses"]));
    assert_eq!(body[0]["thumbnail"], format!("/tn/{HASH}.avif"));
}

#[tokio::test]
async fn gallery_returns_proxied_page_urls() {
    let server = hitomi_stub().await;
    let (status, body) = get_json(app(&server), "/api/gallery/300").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], 300);
    assert_eq!(body["pages"][0]["src"], format!("/img/{HASH}.avif"));
    assert_eq!(body["pages"][0]["width"], 100);
    assert!(
        !body.to_string().contains("gold-usergeneratedcontent"),
        "the browser must never be handed a hitomi URL"
    );
}

#[tokio::test]
async fn format_changes_are_reported_as_such() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/galleriesindex/version"))
        .respond_with(ResponseTemplate::new(200).set_body_string(VERSION))
        .mount(&server)
        .await;
    // 노드 자리에 쓰레기를 돌려준다 — 사이트가 포맷을 바꾼 상황이다.
    Mock::given(method("GET"))
        .and(path(format!("/galleriesindex/galleries.{VERSION}.index")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0xffu8; 464]))
        .mount(&server)
        .await;

    let (status, body) = get_json(app(&server), "/api/search?q=naruto").await;

    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(body["error"], "format_changed");
    assert!(
        body["message"].as_str().unwrap().contains("needs an update"),
        "the message must tell the user what to do: {body}"
    );
}

#[tokio::test]
async fn unreachable_hitomi_is_reported_as_a_network_error() {
    let cfg = Config {
        scheme: "http".into(),
        // 예약된 포트. 연결이 즉시 실패한다.
        ltn_domain: "127.0.0.1:9".into(),
        ..Config::default()
    };
    let fetcher = Arc::new(HttpFetcher::new(FetchConfig::default()).unwrap());
    let app = router(Arc::new(AppState::new(fetcher, cfg)));

    let (status, body) = get_json(app, "/api/search?q=naruto").await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "network");
}

#[tokio::test]
async fn image_proxy_rejects_a_malformed_hash() {
    let server = hitomi_stub().await;
    let (status, body) = get_json(app(&server), "/img/notahash.avif").await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "bad_request");
}

#[tokio::test]
async fn image_proxy_rejects_unsupported_extensions() {
    let server = hitomi_stub().await;
    let (status, _) = get_json(app(&server), &format!("/img/{HASH}.png")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
