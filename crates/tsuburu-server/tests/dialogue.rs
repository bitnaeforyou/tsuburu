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
    let cfg =
        Config { scheme: "http".into(), ltn_domain: "127.0.0.1:9".into(), ..Config::default() };
    Arc::new(Grinder::new(fetcher, cfg, Arc::new(store), Arc::new(ocr)))
}

fn app(grinder: Option<Arc<Grinder>>) -> axum::Router {
    app_with_shards(grinder, None)
}

fn app_with_shards(
    grinder: Option<Arc<Grinder>>,
    shards: Option<std::path::PathBuf>,
) -> axum::Router {
    let fetcher = Arc::new(HttpFetcher::new(FetchConfig::default()).unwrap());
    let cfg =
        Config { scheme: "http".into(), ltn_domain: "127.0.0.1:9".into(), ..Config::default() };
    let mut state = AppState::full(fetcher, cfg, None, grinder);
    if let Some(dir) = shards {
        state = state.with_shards_dir(dir);
    }
    router(Arc::new(state))
}

async fn call(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let builder =
        Request::builder().method(method).uri(uri).header(header::CONTENT_TYPE, "application/json");
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
    let (status, response) =
        call(app(Some(Arc::clone(&g))), "POST", "/api/dialogue/enqueue", Some(body)).await;
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
        let (status, _) =
            call(app(Some(Arc::clone(&g))), "PUT", "/api/dialogue/settings", Some(body)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(g.settings().await.enabled);
        assert_eq!(g.settings().await.kinds, vec!["doujinshi"]);
        // redb allows one open handle per file; release it before reopening.
    }

    // Persisted: a fresh grinder over the same store starts enabled.
    let store = DialogueStore::open(dir.path().join("dialogue.redb")).unwrap();
    let fetcher = Arc::new(HttpFetcher::new(FetchConfig::default()).unwrap());
    let again =
        Grinder::new(fetcher, Config::default(), Arc::new(store), Arc::new(MockOcr::default()));
    assert!(again.settings().await.enabled);
}

#[tokio::test]
async fn search_api_returns_hits_with_counts() {
    let dir = tempfile::tempdir().unwrap();
    let g = grinder(&dir, MockOcr::default());
    g.store().enqueue(&[5], Priority::Background).unwrap();
    g.store()
        .complete(
            5,
            &[tsuburu_dialogue::PageText {
                page: 3,
                lines: vec!["뭐라고".into(), "안녕하세요".into()],
            }],
        )
        .unwrap();

    let (status, body) = call(
        app(Some(g)),
        "GET",
        "/api/dialogue/search?q=%EC%95%88%EB%85%95%ED%95%98%EC%84%B8%EC%9A%94",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["hits"][0]["gallery_id"], 5);
    assert_eq!(body["hits"][0]["page"], 3);
    assert_eq!(body["counts"]["done"], 1);
}

#[tokio::test]
async fn shards_round_trip_between_two_machines_through_the_api() {
    let dir_a = tempfile::tempdir().unwrap();
    let a = grinder(&dir_a, MockOcr::default());
    a.store().enqueue(&[10, 20], Priority::Background).unwrap();
    a.store()
        .complete(
            10,
            &[tsuburu_dialogue::PageText { page: 0, lines: vec!["공유되는 대사".into()] }],
        )
        .unwrap();
    a.store()
        .complete(20, &[tsuburu_dialogue::PageText { page: 0, lines: vec!["또 하나".into()] }])
        .unwrap();

    // Machine A exports.
    let shards_a = dir_a.path().join("shards");
    let (status, body) = call(
        app_with_shards(Some(Arc::clone(&a)), Some(shards_a.clone())),
        "POST",
        "/api/dialogue/export",
        Some(serde_json::json!({ "background_only": true })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let name = body["files"][0]["name"].as_str().unwrap().to_string();
    assert!(name.starts_with("dialogue-0-99999-"), "{name}");
    assert_eq!(body["files"][0]["galleries"], 2);

    // The file can be downloaded...
    let response = app_with_shards(Some(Arc::clone(&a)), Some(shards_a.clone()))
        .oneshot(
            Request::builder()
                .uri(format!("/api/dialogue/shards/{name}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();

    // ...and imported on machine B, which already had one of the two.
    let dir_b = tempfile::tempdir().unwrap();
    let b = grinder(&dir_b, MockOcr::default());
    b.store().enqueue(&[20], Priority::Background).unwrap();
    b.store()
        .complete(20, &[tsuburu_dialogue::PageText { page: 0, lines: vec!["B의 것".into()] }])
        .unwrap();

    let response = app(Some(Arc::clone(&b)))
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/dialogue/import?name={name}"))
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .body(Body::from(bytes.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let summary: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(summary["added"], 1);
    assert_eq!(summary["skipped"], 1);
    assert!(b.store().search("공유되는 대사", 5).unwrap().iter().any(|h| h.gallery_id == 10));
    assert_eq!(b.store().text(20).unwrap().unwrap()[0].lines, vec!["B의 것"]);

    // A tampered file is refused by its name's hash.
    let mut tampered = bytes.to_vec();
    let last = tampered.len() - 1;
    tampered[last] ^= 1;
    let response = app(Some(b))
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/dialogue/import?name={name}"))
                .body(Body::from(tampered))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn shard_download_rejects_names_that_are_not_shards() {
    let dir = tempfile::tempdir().unwrap();
    let g = grinder(&dir, MockOcr::default());
    let (status, _) = call(
        app_with_shards(Some(g), Some(dir.path().join("shards"))),
        "GET",
        // Escaped traversal, spelled without naming a real system file.
        "/api/dialogue/shards/..%2F..%2Felsewhere",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// --- downloads and offline reading ---

/// redb allows one handle per file, so tests open the store once and build
/// each router around that same handle.
fn app_over(downloads: &Arc<tsuburu_downloads::DownloadStore>) -> axum::Router {
    let fetcher = Arc::new(HttpFetcher::new(FetchConfig::default()).unwrap());
    // A dead address: nothing here is allowed to reach the network.
    let cfg =
        Config { scheme: "http".into(), ltn_domain: "127.0.0.1:9".into(), ..Config::default() };
    let state = AppState::full(fetcher, cfg, None, None).with_downloads(Arc::clone(downloads));
    router(Arc::new(state))
}

fn open_downloads(dir: &tempfile::TempDir) -> Arc<tsuburu_downloads::DownloadStore> {
    Arc::new(tsuburu_downloads::DownloadStore::open(dir.path()).unwrap())
}

fn stored(id: i32, hash: &str) -> tsuburu_downloads::Download {
    tsuburu_downloads::Download {
        id,
        title: Some("Downloaded work".into()),
        language: Some("korean".into()),
        kind: Some("manga".into()),
        pages: vec![tsuburu_downloads::DownloadedPage {
            page: 0,
            hash: hash.to_string(),
            ext: "avif".into(),
            width: 100,
            height: 200,
        }],
        added_at: 1,
    }
}

#[tokio::test]
async fn a_downloaded_work_opens_and_reads_with_hitomi_unreachable() {
    let dir = tempfile::tempdir().unwrap();
    let downloads = open_downloads(&dir);
    let hash = "b".repeat(64);
    downloads.put(&stored(77, &hash)).unwrap();
    downloads.save_image(&hash, "avif", b"page-bytes").unwrap();

    let (status, body) = call(app_over(&downloads), "GET", "/api/gallery/77", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["title"], "Downloaded work");
    assert_eq!(body["pages"][0]["src"], format!("/img/{hash}.avif"));

    let response = app_over(&downloads)
        .oneshot(Request::builder().uri(format!("/img/{hash}.avif")).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&bytes[..], b"page-bytes", "served from disk, not relayed");
}

#[tokio::test]
async fn a_gallery_that_was_never_downloaded_still_reports_the_network_failure() {
    let dir = tempfile::tempdir().unwrap();
    let downloads = open_downloads(&dir);
    let (status, body) = call(app_over(&downloads), "GET", "/api/gallery/12345", None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "network");
}

#[tokio::test]
async fn downloads_are_listed_with_progress_and_can_be_removed() {
    let dir = tempfile::tempdir().unwrap();
    let downloads = open_downloads(&dir);
    let hash = "c".repeat(64);
    downloads.put(&stored(9, &hash)).unwrap();

    let (status, body) = call(app_over(&downloads), "GET", "/api/downloads", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"][0]["id"], 9);
    assert_eq!(body["items"][0]["have"], 0);
    assert_eq!(body["items"][0]["complete"], false);

    downloads.save_image(&hash, "avif", b"x").unwrap();
    let (_, body) = call(app_over(&downloads), "GET", "/api/downloads/9", None).await;
    assert_eq!(body["have"], 1);
    assert_eq!(body["complete"], true);

    let (status, body) = call(app_over(&downloads), "DELETE", "/api/downloads/9", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["removed"], true);
    assert!(!downloads.has_image(&hash, "avif"), "its only page goes with it");
}

// --- pages recognised because they were read ---

/// The factory is a plain fn pointer, so what it was asked for is recorded
/// here rather than captured.
static ASKED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

fn recording_ocr(options: tsuburu_ocr::OcrOptions) -> Option<Box<dyn tsuburu_ocr::Ocr>> {
    ASKED.lock().unwrap().push(options.languages.join(","));
    Some(Box::new(MockOcr {
        lines: vec![tsuburu_ocr::Line {
            text: "読んだ".into(),
            confidence: 1.0,
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 0.1,
        }],
        fail: false,
    }))
}

fn reading_grinder(dir: &tempfile::TempDir) -> (Arc<Grinder>, Arc<DialogueStore>) {
    let store = Arc::new(DialogueStore::open(dir.path().join("dialogue.redb")).unwrap());
    let fetcher = Arc::new(HttpFetcher::new(FetchConfig::default()).unwrap());
    let cfg =
        Config { scheme: "http".into(), ltn_domain: "127.0.0.1:9".into(), ..Config::default() };
    let grinder = Arc::new(Grinder::with_factory(
        fetcher,
        cfg,
        Arc::clone(&store),
        // The engine the grinder starts with answers for its own language;
        // another language goes through the factory.
        Arc::new(MockOcr {
            lines: vec![tsuburu_ocr::Line {
                text: "읽은 줄".into(),
                confidence: 1.0,
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 0.1,
            }],
            fail: false,
        }),
        recording_ocr,
    ));
    (grinder, store)
}

#[tokio::test]
async fn a_page_that_was_read_is_recognised_in_the_works_own_language() {
    let dir = tempfile::tempdir().unwrap();
    let (grinder, store) = reading_grinder(&dir);

    grinder.recognise_read_page(42, 3, vec![1, 2, 3], "japanese").await;

    assert_eq!(ASKED.lock().unwrap().last().map(String::as_str), Some("ja-JP"));
    let text = store.text(42).unwrap().unwrap();
    assert_eq!(text.len(), 1);
    assert_eq!(text[0].page, 3);
    assert_eq!(text[0].lines, ["読んだ"]);
    assert!(store.job(42).unwrap().unwrap().from_reading);
}

/// Reading is the only path most readers ever use, and a page that cannot be
/// recognised on it used to leave no trace anywhere: no text, no error, and a
/// dialogue search that said nothing had been read without saying why not.
#[tokio::test]
async fn a_page_that_cannot_be_read_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(DialogueStore::open(dir.path().join("dialogue.redb")).unwrap());
    let fetcher = Arc::new(HttpFetcher::new(FetchConfig::default()).unwrap());
    let grinder = Arc::new(Grinder::new(
        fetcher,
        Config { scheme: "http".into(), ltn_domain: "127.0.0.1:9".into(), ..Config::default() },
        Arc::clone(&store),
        Arc::new(MockOcr { lines: vec![], fail: true }),
    ));

    assert!(grinder.status().await.last_error.is_none());
    grinder.recognise_read_page(44, 0, vec![1, 2, 3], "korean").await;

    let said = grinder.status().await.last_error.expect("the failure is recorded");
    assert!(said.contains("page 1 of 44"), "names the page: {said}");
    assert!(said.contains("mock failure"), "carries the reason: {said}");
    assert!(store.text(44).unwrap().is_none());
}

#[tokio::test]
async fn reading_the_same_page_again_does_not_double_it() {
    let dir = tempfile::tempdir().unwrap();
    let (grinder, store) = reading_grinder(&dir);

    grinder.recognise_read_page(43, 0, vec![1], "korean").await;
    grinder.recognise_read_page(43, 0, vec![1], "korean").await;
    grinder.recognise_read_page(43, 1, vec![1], "korean").await;

    let text = store.text(43).unwrap().unwrap();
    assert_eq!(text.iter().map(|p| p.page).collect::<Vec<_>>(), [0, 1]);
}
