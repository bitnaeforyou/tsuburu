//! The keyword endpoints over a store built from a small graph.csv.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use std::sync::Arc;
use tower::ServiceExt;
use tsuburu_fetch::{FetchConfig, HttpFetcher};
use tsuburu_hitomi::Config;
use tsuburu_keywords::KeywordStore;
use tsuburu_server::{AppState, router};

const HEAD: &str = "article_id,rank,keyword,score,tf,df,total_pages,dialogue_count,char_count\n";

/// redb opens a file once per process, so every router shares one handle.
fn app_over(keywords: Option<&Arc<KeywordStore>>) -> axum::Router {
    let fetcher = Arc::new(HttpFetcher::new(FetchConfig::default()).unwrap());
    let cfg =
        Config { scheme: "http".into(), ltn_domain: "127.0.0.1:9".into(), ..Config::default() };
    let mut state = AppState::full(fetcher, cfg, None, None);
    if let Some(store) = keywords {
        state = state.with_keywords(Arc::clone(store));
    }
    router(Arc::new(state))
}

fn imported(dir: &tempfile::TempDir) -> Arc<KeywordStore> {
    let csv = dir.path().join("graph.csv");
    std::fs::write(
        &csv,
        format!(
            "{HEAD}\
             10,1,에미,20.0,7,20,157,1702,8542\n\
             10,2,보스,10.0,3,40,157,1702,8542\n\
             20,1,에미,18.0,7,20,157,1702,8542\n\
             20,2,보스,12.0,3,40,157,1702,8542\n\
             30,1,교사,30.0,7,15,157,1702,8542\n"
        ),
    )
    .unwrap();
    let store = KeywordStore::open(dir.path().join("keywords.redb")).unwrap();
    tsuburu_keywords::graph::import(&store, &csv, &mut |_| {}).unwrap();
    Arc::new(store)
}

async fn get(app: axum::Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let request = Request::builder()
        .method("GET")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
async fn a_work_reports_what_it_is_about() {
    let dir = tempfile::tempdir().unwrap();
    let store = imported(&dir);
    let (status, body) = get(app_over(Some(&store)), "/api/keywords/10").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["words"][0]["word"], "에미");
    assert!(body["words"][0]["score"].as_f64().unwrap() > 19.0);
    assert_eq!(body["words"][1]["word"], "보스");
}

#[tokio::test]
async fn a_work_with_no_keywords_answers_with_an_empty_list() {
    let dir = tempfile::tempdir().unwrap();
    let store = imported(&dir);
    let (status, body) = get(app_over(Some(&store)), "/api/keywords/999").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["words"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn works_about_the_same_things_are_found_and_the_shared_words_named() {
    let dir = tempfile::tempdir().unwrap();
    let store = imported(&dir);
    let (status, body) = get(app_over(Some(&store)), "/api/keywords/10/near").await;
    assert_eq!(status, StatusCode::OK);
    let near = body.as_array().unwrap();
    assert_eq!(near.len(), 1, "{body}");
    assert_eq!(near[0]["id"], 20);
    assert_eq!(near[0]["shared"][0], "에미");
    // 30 shares nothing with 10, so it is absent rather than last.
    assert!(near.iter().all(|n| n["id"] != 30));
}

#[tokio::test]
async fn searching_a_word_ranks_the_work_it_belongs_to_most() {
    let dir = tempfile::tempdir().unwrap();
    let store = imported(&dir);
    let (status, body) =
        get(app_over(Some(&store)), "/api/keywords/search?q=%EC%97%90%EB%AF%B8").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["word"], "에미");
    let works = body["works"].as_array().unwrap();
    assert_eq!(works[0]["id"], 10);
    assert_eq!(works[1]["id"], 20);
}

#[tokio::test]
async fn an_empty_word_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store = imported(&dir);
    let (status, _) = get(app_over(Some(&store)), "/api/keywords/search?q=%20").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn without_an_import_the_endpoints_say_so_rather_than_failing_blankly() {
    let (status, body) = get(app_over(None), "/api/keywords/10").await;
    assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    assert!(body["message"].as_str().unwrap().contains("import-keywords"), "{body}");
}
