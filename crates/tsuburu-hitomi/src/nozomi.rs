//! `.nozomi` 목록 — hitomi가 이미 정렬해둔 갤러리 ID 배열.
//!
//! 파일마다 정렬 기준과 필터가 다르고, 언어는 **파일 이름에 들어 있다.**
//! 그래서 언어 필터는 추가 연산 없이 공짜다.
//!
//! 내용은 빅엔디언 i32 배열이고 그 외의 구조가 없다. 앞의 25개만 필요하면
//! 100바이트만 읽으면 된다.

use crate::Config;
use crate::fetcher::Fetcher;
use crate::index::SearchError;
use std::collections::HashSet;

/// 한 번에 읽을 바이트. 인기순 재정렬에서 목록을 훑을 때 쓴다.
/// 64 KB면 ID 16,384개다.
const SCAN_CHUNK: u64 = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    /// 최신순. 검색 결과도 이미 이 순서라 추가 비용이 없다.
    #[default]
    Date,
    PopularToday,
    PopularWeek,
    PopularMonth,
    PopularYear,
}

impl Sort {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "date" => Sort::Date,
            "today" => Sort::PopularToday,
            "week" => Sort::PopularWeek,
            "month" => Sort::PopularMonth,
            "year" => Sort::PopularYear,
            _ => return None,
        })
    }

    fn path(self, language: &str) -> String {
        match self {
            Sort::Date => format!("index-{language}.nozomi"),
            Sort::PopularToday => format!("popular/today-{language}.nozomi"),
            Sort::PopularWeek => format!("popular/week-{language}.nozomi"),
            Sort::PopularMonth => format!("popular/month-{language}.nozomi"),
            Sort::PopularYear => format!("popular/year-{language}.nozomi"),
        }
    }

    pub fn is_date(self) -> bool {
        matches!(self, Sort::Date)
    }
}

/// 언어와 종류 필터. 둘 다 `.nozomi` 파일로 표현된다.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filters {
    /// `None`이면 전체(`all`).
    pub language: Option<String>,
    /// `doujinshi`, `manga`, `artistcg` 등. `None`이면 전체.
    pub kind: Option<String>,
}

impl Filters {
    pub fn language_or_all(&self) -> &str {
        self.language.as_deref().unwrap_or("all")
    }

    pub fn is_empty(&self) -> bool {
        self.language.is_none() && self.kind.is_none()
    }
}

impl Config {
    fn nozomi_url(&self, path: &str) -> String {
        format!("{}://{}/{path}", self.scheme, self.ltn_domain)
    }

    pub fn sort_list_url(&self, sort: Sort, language: &str) -> String {
        self.nozomi_url(&sort.path(language))
    }

    pub fn type_list_url(&self, kind: &str, language: &str) -> String {
        self.nozomi_url(&format!("type/{kind}-{language}.nozomi"))
    }

    /// One artist's or one series' works, as hitomi itself lists them.
    ///
    /// The search index has no `artist:` key - asking it for one finds
    /// nothing - but these lists exist beside it, one file per name per
    /// language. The name goes in as written and lowercased, with its spaces
    /// percent-encoded: `amano-ameno` and `amanoameno` are both 404.
    pub fn name_list_url(&self, namespace: &str, name: &str, language: &str) -> String {
        let escaped = name.to_lowercase().replace(' ', "%20");
        self.nozomi_url(&format!("{namespace}/{escaped}-{language}.nozomi"))
    }
}

pub fn decode_ids(bytes: &[u8]) -> Vec<i32> {
    bytes.as_chunks::<4>().0.iter().copied().map(i32::from_be_bytes).collect()
}

/// 목록에 담긴 갤러리 수.
pub async fn count(fetcher: &dyn Fetcher, url: &str) -> Result<usize, SearchError> {
    Ok(fetcher.length(url).await? as usize / 4)
}

/// 목록의 한 페이지. 필요한 구간만 Range로 읽는다.
pub async fn page(
    fetcher: &dyn Fetcher,
    url: &str,
    offset: usize,
    limit: usize,
) -> Result<Vec<i32>, SearchError> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let start = offset as u64 * 4;
    let end = start + limit as u64 * 4;
    Ok(decode_ids(&fetcher.get_range(url, start..end).await?))
}

/// 목록 전체를 집합으로. 필터로 쓸 때만 사용한다.
///
/// 목록에 따라 수 MB가 될 수 있다. 한 번 받으면 캐시되지만 첫 요청은 느리다.
/// The whole of a `.nozomi` list, undecoded, for a caller that wants the ids
/// in some other shape than a set.
pub async fn all_ids(fetcher: &dyn Fetcher, url: &str) -> Result<Vec<u8>, SearchError> {
    let total = fetcher.length(url).await?;
    Ok(fetcher.get_range(url, 0..total).await?)
}

pub async fn id_set(fetcher: &dyn Fetcher, url: &str) -> Result<HashSet<i32>, SearchError> {
    let total = fetcher.length(url).await?;
    let bytes = fetcher.get_range(url, 0..total).await?;
    Ok(decode_ids(&bytes).into_iter().collect())
}

/// 정렬 목록을 훑으며 `allowed`에 있는 ID만 순서대로 모은다.
///
/// 결과가 목록 전체에 고르게 흩어져 있으면 싸다. 결과가 5,000건이고 목록이
/// 950,000건이면 25개를 채우는 데 20 KB쯤 읽는다. 결과가 좁으면 목록을 거의
/// 다 훑게 되는데, 그 비용은 감수한다(스펙 3.2절). 청크 단위로 읽으므로 원하는
/// 만큼 모이면 즉시 멈춘다.
pub async fn reorder(
    fetcher: &dyn Fetcher,
    url: &str,
    allowed: &HashSet<i32>,
    hidden: &HashSet<i32>,
    offset: usize,
    limit: usize,
) -> Result<Vec<i32>, SearchError> {
    if allowed.is_empty() {
        return Ok(Vec::new());
    }
    walk(fetcher, url, offset, limit, |id| allowed.contains(&id) && !hidden.contains(&id)).await
}

/// 목록을 순서대로 훑으며 `hidden`에 든 ID만 건너뛴다.
///
/// Browsing with nothing hidden reads only the bytes for the page asked for -
/// a hundred of them for twenty-five works. A reader who has hidden something
/// cannot be served that way, because what to skip is only known by looking,
/// so the list is walked in chunks and stops as soon as the page is full. A
/// tag covering a twentieth of the catalogue costs a twentieth more reading.
pub async fn page_without(
    fetcher: &dyn Fetcher,
    url: &str,
    hidden: &HashSet<i32>,
    offset: usize,
    limit: usize,
) -> Result<Vec<i32>, SearchError> {
    walk(fetcher, url, offset, limit, |id| !hidden.contains(&id)).await
}

async fn walk(
    fetcher: &dyn Fetcher,
    url: &str,
    offset: usize,
    limit: usize,
    keep: impl Fn(i32) -> bool,
) -> Result<Vec<i32>, SearchError> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let total_bytes = fetcher.length(url).await?;
    let wanted = offset + limit;

    let mut matched = Vec::with_capacity(wanted.min(1024));
    let mut at = 0u64;
    while at < total_bytes && matched.len() < wanted {
        let end = (at + SCAN_CHUNK).min(total_bytes);
        let chunk = fetcher.get_range(url, at..end).await?;
        if chunk.is_empty() {
            break;
        }
        for id in decode_ids(&chunk) {
            if keep(id) {
                matched.push(id);
                if matched.len() >= wanted {
                    break;
                }
            }
        }
        at = end;
    }

    Ok(matched.into_iter().skip(offset).collect())
}

/// 목록에서 숨긴 것을 뺀 개수.
///
/// 전체를 훑어야 하므로 목록 하나를 통째로 읽는다. 캐시에 얹히니 페이지를
/// 넘길 때마다 다시 읽지는 않는다.
pub async fn count_without(
    fetcher: &dyn Fetcher,
    url: &str,
    hidden: &HashSet<i32>,
) -> Result<usize, SearchError> {
    if hidden.is_empty() {
        return count(fetcher, url).await;
    }
    let bytes = all_ids(fetcher, url).await?;
    Ok(decode_ids(&bytes).into_iter().filter(|id| !hidden.contains(id)).count())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetcher::mock::MockFetcher;

    fn nozomi(ids: &[i32]) -> Vec<u8> {
        ids.iter().flat_map(|id| id.to_be_bytes()).collect()
    }

    #[test]
    fn sort_paths_embed_the_language() {
        assert_eq!(Sort::Date.path("korean"), "index-korean.nozomi");
        assert_eq!(Sort::PopularWeek.path("all"), "popular/week-all.nozomi");
    }

    #[test]
    fn sort_parses_known_names_only() {
        assert_eq!(Sort::parse("week"), Some(Sort::PopularWeek));
        assert_eq!(Sort::parse("date"), Some(Sort::Date));
        assert_eq!(Sort::parse("nonsense"), None);
    }

    #[test]
    fn urls_come_from_config() {
        let cfg = Config::default();
        assert_eq!(
            cfg.sort_list_url(Sort::PopularToday, "korean"),
            "https://ltn.gold-usergeneratedcontent.net/popular/today-korean.nozomi"
        );
        assert_eq!(
            cfg.type_list_url("manga", "all"),
            "https://ltn.gold-usergeneratedcontent.net/type/manga-all.nozomi"
        );
    }

    #[tokio::test]
    async fn count_divides_the_length() {
        let f = MockFetcher::with("l", nozomi(&[1, 2, 3]));
        assert_eq!(count(&f, "l").await.unwrap(), 3);
    }

    #[tokio::test]
    async fn page_reads_only_the_requested_span() {
        let f = MockFetcher::with("l", nozomi(&[10, 20, 30, 40, 50]));
        assert_eq!(page(&f, "l", 1, 2).await.unwrap(), vec![20, 30]);

        let (_, range) = f.calls.lock().unwrap()[0].clone();
        assert_eq!((range.start, range.end), (4, 12));
    }

    #[tokio::test]
    async fn page_past_the_end_is_empty() {
        let f = MockFetcher::with("l", nozomi(&[1, 2]));
        assert!(page(&f, "l", 10, 5).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn reorder_follows_the_list_order_not_the_set() {
        // 목록은 인기순, allowed는 검색 결과라고 하자.
        let f = MockFetcher::with("pop", nozomi(&[5, 9, 1, 7, 3]));
        let allowed: HashSet<i32> = [1, 3, 5].into_iter().collect();
        assert_eq!(
            reorder(&f, "pop", &allowed, &HashSet::new(), 0, 10).await.unwrap(),
            vec![5, 1, 3]
        );
    }

    #[tokio::test]
    async fn reorder_honours_offset_and_limit() {
        let f = MockFetcher::with("pop", nozomi(&[5, 9, 1, 7, 3]));
        let allowed: HashSet<i32> = [1, 3, 5].into_iter().collect();
        assert_eq!(reorder(&f, "pop", &allowed, &HashSet::new(), 1, 1).await.unwrap(), vec![1]);
    }

    #[tokio::test]
    async fn reorder_stops_early_once_the_page_is_full() {
        // 16,384개보다 긴 목록을 만들어 청크가 여러 개 되게 한다.
        let ids: Vec<i32> = (0..40_000).rev().collect();
        let f = MockFetcher::with("pop", nozomi(&ids));
        // 맨 앞에 몰려 있는 결과 -> 첫 청크에서 끝나야 한다
        let allowed: HashSet<i32> = (39_990..40_000).collect();

        let got = reorder(&f, "pop", &allowed, &HashSet::new(), 0, 5).await.unwrap();
        assert_eq!(got.len(), 5);
        // length 1회 + range 1회. 두 번째 청크까지 갔다면 3회가 된다.
        assert_eq!(f.call_count(), 1, "must not scan past the first chunk");
    }

    #[tokio::test]
    async fn reorder_with_an_empty_set_does_no_work() {
        let f = MockFetcher::with("pop", nozomi(&[1, 2, 3]));
        assert!(
            reorder(&f, "pop", &HashSet::new(), &HashSet::new(), 0, 10).await.unwrap().is_empty()
        );
        assert_eq!(f.call_count(), 0);
    }

    #[tokio::test]
    async fn id_set_reads_everything() {
        let f = MockFetcher::with("l", nozomi(&[4, 8, 15]));
        let set = id_set(&f, "l").await.unwrap();
        assert_eq!(set, [4, 8, 15].into_iter().collect());
    }

    #[tokio::test]
    async fn what_is_hidden_is_left_out_of_a_reordering() {
        let f = MockFetcher::with("pop", nozomi(&[5, 4, 1, 3, 2]));
        let allowed: HashSet<i32> = [1, 3, 5].into_iter().collect();
        let hidden: HashSet<i32> = [3].into_iter().collect();
        assert_eq!(reorder(&f, "pop", &allowed, &hidden, 0, 10).await.unwrap(), vec![5, 1]);
    }

    #[tokio::test]
    async fn browsing_skips_what_is_hidden_and_still_fills_the_page() {
        let f = MockFetcher::with("list", nozomi(&[9, 8, 7, 6, 5, 4, 3, 2, 1]));
        let hidden: HashSet<i32> = [8, 6, 4].into_iter().collect();
        assert_eq!(
            page_without(&f, "list", &hidden, 0, 3).await.unwrap(),
            vec![9, 7, 5],
            "a page is three works, not three minus the hidden ones"
        );
        assert_eq!(page_without(&f, "list", &hidden, 3, 3).await.unwrap(), vec![3, 2, 1]);
        assert_eq!(count_without(&f, "list", &hidden).await.unwrap(), 6);
    }

    #[tokio::test]
    async fn hiding_nothing_reads_only_the_page_asked_for() {
        let f = MockFetcher::with("list", nozomi(&[9, 8, 7, 6, 5]));
        assert_eq!(count_without(&f, "list", &HashSet::new()).await.unwrap(), 5);
        // `count` asks for the length alone; with something hidden it would
        // have had to read the whole list to know what was left.
        assert_eq!(f.call_count(), 0, "a length is not a read");
    }
}
