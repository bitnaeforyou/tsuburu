//! 실제 hitomi에 붙는 통합 테스트.
//!
//! 네트워크와 외부 사이트 상태에 의존하므로 기본 실행에서 제외한다.
//! `cargo test -p tsuburu-fetch -- --ignored`로 실행한다.
//!
//! 이 테스트가 깨지면 사이트 포맷이 바뀐 것이다.

use tsuburu_fetch::{FetchConfig, HttpFetcher};
use tsuburu_hitomi::{Config, parse_query};

fn setup() -> (HttpFetcher, Config) {
    (
        HttpFetcher::new(FetchConfig::default()).unwrap(),
        Config::default(),
    )
}

#[tokio::test]
#[ignore = "requires network access to hitomi"]
async fn searches_real_index() {
    let (fetcher, cfg) = setup();
    let version = tsuburu_hitomi::galleries_index_version(&fetcher, &cfg)
        .await
        .unwrap();

    let ids = tsuburu_hitomi::search_term(&fetcher, &cfg, &version, "naruto", Some(10))
        .await
        .unwrap();

    assert!(!ids.is_empty(), "naruto must match galleries");
    assert!(ids.len() <= 10);
    // 결과는 날짜순이지 ID순이 아니다. 같은 시각에 올라온 갤러리는 ID가
    // 뒤바뀌므로 엄격한 내림차순을 요구하면 안 된다.
    let descending = ids.windows(2).filter(|w| w[0] > w[1]).count();
    assert!(descending * 2 >= ids.len(), "results should still be roughly newest-first");
}

#[tokio::test]
#[ignore = "requires network access to hitomi"]
async fn intersects_two_terms() {
    let (fetcher, cfg) = setup();
    let version = tsuburu_hitomi::galleries_index_version(&fetcher, &cfg)
        .await
        .unwrap();

    let query = parse_query("glasses school");
    let both = tsuburu_hitomi::search(&fetcher, &cfg, &version, &query, None)
        .await
        .unwrap();
    let single = tsuburu_hitomi::search_term(&fetcher, &cfg, &version, "glasses", None)
        .await
        .unwrap();

    assert!(!both.is_empty(), "these common tags must overlap");
    assert!(both.len() < single.len(), "AND must narrow the result");
    // 회귀 방지: 정렬을 전제한 교집합은 두 흔한 태그의 겹침을 1 % 미만으로
    // 줄여버렸다. 실제 겹침은 절반 남짓이다.
    assert!(
        both.len() * 10 > single.len(),
        "two common tags should overlap substantially, got {} of {}",
        both.len(),
        single.len()
    );
}

#[tokio::test]
#[ignore = "requires network access to hitomi"]
async fn cache_removes_roundtrips_on_repeat() {
    let (fetcher, cfg) = setup();
    let version = tsuburu_hitomi::galleries_index_version(&fetcher, &cfg)
        .await
        .unwrap();

    tsuburu_hitomi::search_term(&fetcher, &cfg, &version, "naruto", Some(5))
        .await
        .unwrap();
    let after_first = fetcher.stats().requests;

    tsuburu_hitomi::search_term(&fetcher, &cfg, &version, "naruto", Some(5))
        .await
        .unwrap();
    let after_second = fetcher.stats().requests;

    assert_eq!(
        after_first, after_second,
        "a repeated identical search must be served entirely from cache"
    );
}

#[tokio::test]
#[ignore = "requires network access to hitomi"]
async fn fetches_gallery_and_builds_image_url() {
    let (fetcher, cfg) = setup();
    let version = tsuburu_hitomi::galleries_index_version(&fetcher, &cfg)
        .await
        .unwrap();
    let ids = tsuburu_hitomi::search_term(&fetcher, &cfg, &version, "naruto", Some(1))
        .await
        .unwrap();

    let gallery = tsuburu_hitomi::fetch_gallery(&fetcher, &cfg, ids[0])
        .await
        .unwrap();
    assert!(!gallery.files.is_empty());

    let gg = tsuburu_hitomi::fetch_gg(&fetcher, &cfg).await.unwrap();
    let url = tsuburu_hitomi::image_url(&cfg, &gg, &gallery.files[0]).unwrap();

    // 실제로 이미지가 받아지는지 확인한다. Referer가 빠지면 여기서 실패한다.
    let response = fetcher.stream(&url).await.unwrap();
    let bytes = response.bytes().await.unwrap();
    assert!(bytes.len() > 1000, "image at {url} was {} bytes", bytes.len());
}
