//! hitomi 프로토콜 지식을 독점하는 crate.
//!
//! **불변 규칙:** 바이트 오프셋, 도메인 문자열, URL 규칙은 이 crate 밖으로
//! 나가지 않는다. 사이트가 바뀌었을 때 고칠 범위를 여기로 가두기 위한 것이다.
//!
//! 이 crate는 네트워크를 하지 않는다. [`Fetcher`] 트레잇에만 의존하므로 저장된
//! 응답 바이트만으로 전부 테스트할 수 있다.

pub mod fetcher;
pub mod gallery;
pub mod image;
pub mod index;
pub mod node;
pub mod nozomi;
pub mod search;

pub use fetcher::{FetchError, Fetcher};
pub use gallery::{Gallery, GalleryError, GalleryFile, parse_gallery_info};
pub use image::{GgMap, ImageError, image_url, parse_gg, thumbnail_url};
pub use index::{
    IdBlock, MAX_NODE_SIZE, SearchError, b_search, fetch_gallery_id_block, fetch_gallery_ids,
    hash_term, warm_index,
};
pub use node::{Node, decode_node};
pub use nozomi::{Filters, Sort};
pub use search::{Query, difference, intersect, parse_query};

/// B-tree 차수. 노드는 자식 주소를 `B + 1`개 담는다.
pub const B: usize = 16;

/// 접속 대상. `ltn.hitomi.la`가 `ltn.gold-usergeneratedcontent.net`으로 옮겨간
/// 전례가 있으므로 상수가 아니라 설정으로 둔다.
#[derive(Debug, Clone)]
pub struct Config {
    /// 테스트에서 로컬 목 서버를 가리키기 위해 열어둔다. 기본은 `https`다.
    pub scheme: String,
    pub ltn_domain: String,
    pub content_domain: String,
    pub referer: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            scheme: "https".into(),
            ltn_domain: "ltn.gold-usergeneratedcontent.net".into(),
            content_domain: "gold-usergeneratedcontent.net".into(),
            referer: "https://hitomi.la/".into(),
        }
    }
}

impl Config {
    pub fn galleries_index_url(&self, version: &str) -> String {
        format!("{}://{}/galleriesindex/galleries.{version}.index", self.scheme, self.ltn_domain)
    }

    pub fn galleries_data_url(&self, version: &str) -> String {
        format!("{}://{}/galleriesindex/galleries.{version}.data", self.scheme, self.ltn_domain)
    }

    pub fn version_url(&self, index_dir: &str, cache_buster: u64) -> String {
        format!("{}://{}/{index_dir}/version?_={cache_buster}", self.scheme, self.ltn_domain)
    }

    pub fn gallery_info_url(&self, id: i32) -> String {
        format!("{}://{}/galleries/{id}.js", self.scheme, self.ltn_domain)
    }

    pub fn gg_url(&self) -> String {
        format!("{}://{}/gg.js", self.scheme, self.ltn_domain)
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 갤러리 인덱스의 현재 버전 문자열.
pub async fn galleries_index_version(
    fetcher: &dyn Fetcher,
    cfg: &Config,
) -> Result<String, SearchError> {
    let url = cfg.version_url("galleriesindex", now_secs());
    let body = fetcher.get(&url).await?;
    let text = String::from_utf8_lossy(&body).trim().to_string();
    if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
        return Err(SearchError::BlockLength { got: text.len(), want: 0 });
    }
    Ok(text)
}

/// 검색어 하나에 해당하는 갤러리 ID 목록. 내림차순(최신순)이다.
///
/// **인덱스는 낱말 단위다.** `big breasts`라는 키는 없고 `big`과 `breasts`가
/// 따로 있다(실측). 그래서 여러 낱말로 된 검색어는 각 낱말을 찾아 교집합을
/// 취한다. 한국어 사전이 `거유 -> big breasts`처럼 여러 낱말을 내놓기 때문에
/// 이 처리가 없으면 결과가 거의 나오지 않는다.
pub async fn search_term(
    fetcher: &dyn Fetcher,
    cfg: &Config,
    version: &str,
    term: &str,
    limit: Option<usize>,
) -> Result<Vec<i32>, SearchError> {
    let words: Vec<&str> = term.split_whitespace().collect();
    match words.as_slice() {
        [] => Ok(Vec::new()),
        [single] => search_word(fetcher, cfg, version, single, limit).await,
        many => {
            // 교집합을 취해야 하므로 각 낱말의 전체 목록이 필요하다.
            let mut acc: Option<Vec<i32>> = None;
            for word in many {
                let ids = search_word(fetcher, cfg, version, word, None).await?;
                acc = Some(match acc {
                    None => ids,
                    Some(prev) => intersect(&prev, &ids),
                });
                if acc.as_ref().is_some_and(|v| v.is_empty()) {
                    return Ok(Vec::new());
                }
            }
            let mut result = acc.unwrap_or_default();
            if let Some(n) = limit {
                result.truncate(n);
            }
            Ok(result)
        }
    }
}

/// 낱말 하나. 인덱스 키와 1:1로 대응한다.
async fn search_word(
    fetcher: &dyn Fetcher,
    cfg: &Config,
    version: &str,
    word: &str,
    limit: Option<usize>,
) -> Result<Vec<i32>, SearchError> {
    let key = hash_term(&word.to_lowercase());
    let index_url = cfg.galleries_index_url(version);
    let Some(entry) = b_search(fetcher, &index_url, &key).await? else {
        return Ok(Vec::new());
    };
    let data_url = cfg.galleries_data_url(version);
    fetch_gallery_ids(fetcher, &data_url, entry, limit).await
}

/// 한 페이지 분량의 검색 결과.
#[derive(Debug, Clone)]
pub struct SearchPage {
    /// 조건에 맞는 전체 개수.
    pub total: usize,
    pub ids: Vec<i32>,
}

/// 한 번의 검색 요청.
#[derive(Debug, Clone, Copy)]
pub struct SearchRequest<'a> {
    pub query: &'a Query,
    pub filters: &'a Filters,
    pub sort: Sort,
    pub offset: usize,
    pub limit: usize,
}

/// 페이지 단위 검색. 정렬과 필터를 함께 받는다.
///
/// 경로가 둘로 갈린다(스펙 3.1절).
///
/// - **검색어가 없으면** hitomi가 이미 정렬해둔 `.nozomi` 목록을 그대로 페이징
///   한다. 첫 페이지에 100바이트 남짓만 읽으므로 검색보다도 싸다.
/// - **검색어가 있으면** B-tree로 ID 집합을 얻고, 필요한 필터를 교집합으로
///   접은 뒤, 인기순을 요청한 경우에만 정렬 목록을 훑어 순서를 바꾼다.
pub async fn search_page(
    fetcher: &dyn Fetcher,
    cfg: &Config,
    version: &str,
    request: &SearchRequest<'_>,
) -> Result<SearchPage, SearchError> {
    let SearchRequest { query, filters, sort, offset, limit } = *request;
    let language = filters.language_or_all();

    if query.include.is_empty() {
        return browse(fetcher, cfg, filters, sort, offset, limit).await;
    }

    // 검색어가 하나뿐이고 필터도 정렬도 기본이면, 데이터 블록 앞부분만 읽는
    // 기존의 값싼 경로를 그대로 쓴다.
    let single_word = query.include.len() == 1
        && !query.include[0].contains(char::is_whitespace)
        && query.exclude.is_empty();
    if single_word && filters.is_empty() && sort.is_date() {
        let term = &query.include[0];
        let key = hash_term(&term.to_lowercase());
        let index_url = cfg.galleries_index_url(version);
        let Some(entry) = b_search(fetcher, &index_url, &key).await? else {
            return Ok(SearchPage { total: 0, ids: Vec::new() });
        };
        let data_url = cfg.galleries_data_url(version);
        let block = fetch_gallery_id_block(fetcher, &data_url, entry, Some(offset + limit)).await?;
        let ids = block.ids.into_iter().skip(offset).take(limit).collect();
        return Ok(SearchPage { total: block.total, ids });
    }

    let mut ids = search(fetcher, cfg, version, query, None).await?;

    // 언어 필터는 날짜순 언어별 목록의 소속 여부로 판단한다.
    if filters.language.is_some() {
        let url = cfg.sort_list_url(Sort::Date, language);
        let allowed = nozomi::id_set(fetcher, &url).await?;
        ids.retain(|id| allowed.contains(id));
    }
    if let Some(kind) = &filters.kind {
        let url = cfg.type_list_url(kind, language);
        let allowed = nozomi::id_set(fetcher, &url).await?;
        ids.retain(|id| allowed.contains(id));
    }

    let total = ids.len();
    if sort.is_date() {
        // B-tree 결과와 nozomi 목록은 둘 다 최신순이라 이미 정렬돼 있다.
        let page = ids.into_iter().skip(offset).take(limit).collect();
        return Ok(SearchPage { total, ids: page });
    }

    let allowed: std::collections::HashSet<i32> = ids.into_iter().collect();
    let url = cfg.sort_list_url(sort, language);
    let page = nozomi::reorder(fetcher, &url, &allowed, offset, limit).await?;
    Ok(SearchPage { total, ids: page })
}

/// 검색어 없이 둘러보기. 정렬 목록을 직접 페이징한다.
async fn browse(
    fetcher: &dyn Fetcher,
    cfg: &Config,
    filters: &Filters,
    sort: Sort,
    offset: usize,
    limit: usize,
) -> Result<SearchPage, SearchError> {
    let language = filters.language_or_all();

    let Some(kind) = &filters.kind else {
        let url = cfg.sort_list_url(sort, language);
        let total = nozomi::count(fetcher, &url).await?;
        let ids = nozomi::page(fetcher, &url, offset, limit).await?;
        return Ok(SearchPage { total, ids });
    };

    // 종류별 목록도 최신순이므로, 날짜순이면 그 목록을 그대로 페이징하면 된다.
    let type_url = cfg.type_list_url(kind, language);
    let total = nozomi::count(fetcher, &type_url).await?;
    if sort.is_date() {
        let ids = nozomi::page(fetcher, &type_url, offset, limit).await?;
        return Ok(SearchPage { total, ids });
    }

    let allowed = nozomi::id_set(fetcher, &type_url).await?;
    let sort_url = cfg.sort_list_url(sort, language);
    let ids = nozomi::reorder(fetcher, &sort_url, &allowed, offset, limit).await?;
    Ok(SearchPage { total, ids })
}

/// 복합 질의. include는 교집합, exclude는 차집합이다.
///
/// include가 하나뿐이면 데이터 블록을 부분만 읽을 수 있으므로 `limit`을
/// 넘긴다. 여럿이면 교집합에 전체 목록이 필요하다.
pub async fn search(
    fetcher: &dyn Fetcher,
    cfg: &Config,
    version: &str,
    query: &Query,
    limit: Option<usize>,
) -> Result<Vec<i32>, SearchError> {
    if query.include.is_empty() {
        return Ok(Vec::new());
    }

    let single = query.include.len() == 1 && query.exclude.is_empty();
    let per_term_limit = if single { limit } else { None };

    let mut acc: Option<Vec<i32>> = None;
    for term in &query.include {
        let ids = search_term(fetcher, cfg, version, term, per_term_limit).await?;
        acc = Some(match acc {
            None => ids,
            Some(prev) => intersect(&prev, &ids),
        });
        if acc.as_ref().is_some_and(|v| v.is_empty()) {
            return Ok(Vec::new());
        }
    }

    let mut result = acc.unwrap_or_default();
    for term in &query.exclude {
        let ids = search_term(fetcher, cfg, version, term, None).await?;
        result = difference(&result, &ids);
    }

    if let Some(n) = limit {
        result.truncate(n);
    }
    Ok(result)
}

/// 갤러리 메타데이터.
pub async fn fetch_gallery(
    fetcher: &dyn Fetcher,
    cfg: &Config,
    id: i32,
) -> Result<Gallery, GalleryFetchError> {
    let body = fetcher.get(&cfg.gallery_info_url(id)).await?;
    Ok(parse_gallery_info(&String::from_utf8_lossy(&body))?)
}

/// 이미지 URL 매핑 테이블.
pub async fn fetch_gg(fetcher: &dyn Fetcher, cfg: &Config) -> Result<GgMap, GalleryFetchError> {
    let body = fetcher.get(&cfg.gg_url()).await?;
    Ok(parse_gg(&String::from_utf8_lossy(&body))?)
}

#[derive(Debug, thiserror::Error)]
pub enum GalleryFetchError {
    #[error(transparent)]
    Fetch(#[from] FetchError),
    #[error(transparent)]
    Gallery(#[from] GalleryError),
    #[error(transparent)]
    Image(#[from] ImageError),
}

impl GalleryFetchError {
    pub fn is_format_error(&self) -> bool {
        !matches!(self, GalleryFetchError::Fetch(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_index_urls_from_config() {
        let cfg = Config::default();
        assert_eq!(
            cfg.galleries_index_url("123"),
            "https://ltn.gold-usergeneratedcontent.net/galleriesindex/galleries.123.index"
        );
        assert_eq!(
            cfg.galleries_data_url("123"),
            "https://ltn.gold-usergeneratedcontent.net/galleriesindex/galleries.123.data"
        );
        assert_eq!(
            cfg.gallery_info_url(42),
            "https://ltn.gold-usergeneratedcontent.net/galleries/42.js"
        );
    }

    #[test]
    fn config_domain_is_overridable() {
        let cfg = Config { ltn_domain: "example.test".into(), ..Config::default() };
        assert!(cfg.gg_url().starts_with("https://example.test/"));

        let local =
            Config { scheme: "http".into(), ltn_domain: "127.0.0.1:9".into(), ..Config::default() };
        assert!(local.gg_url().starts_with("http://127.0.0.1:9/"));
    }
}
