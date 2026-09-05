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
pub mod search;

pub use fetcher::{FetchError, Fetcher};
pub use gallery::{Gallery, GalleryError, GalleryFile, parse_gallery_info};
pub use image::{GgMap, ImageError, image_url, parse_gg};
pub use index::{MAX_NODE_SIZE, SearchError, b_search, fetch_gallery_ids, hash_term};
pub use node::{Node, decode_node};
pub use search::{Query, difference, intersect, parse_query};

/// B-tree 차수. 노드는 자식 주소를 `B + 1`개 담는다.
pub const B: usize = 16;

/// 접속 대상. `ltn.hitomi.la`가 `ltn.gold-usergeneratedcontent.net`으로 옮겨간
/// 전례가 있으므로 상수가 아니라 설정으로 둔다.
#[derive(Debug, Clone)]
pub struct Config {
    pub ltn_domain: String,
    pub content_domain: String,
    pub referer: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ltn_domain: "ltn.gold-usergeneratedcontent.net".into(),
            content_domain: "gold-usergeneratedcontent.net".into(),
            referer: "https://hitomi.la/".into(),
        }
    }
}

impl Config {
    pub fn galleries_index_url(&self, version: &str) -> String {
        format!("https://{}/galleriesindex/galleries.{version}.index", self.ltn_domain)
    }

    pub fn galleries_data_url(&self, version: &str) -> String {
        format!("https://{}/galleriesindex/galleries.{version}.data", self.ltn_domain)
    }

    pub fn version_url(&self, index_dir: &str, cache_buster: u64) -> String {
        format!("https://{}/{index_dir}/version?_={cache_buster}", self.ltn_domain)
    }

    pub fn gallery_info_url(&self, id: i32) -> String {
        format!("https://{}/galleries/{id}.js", self.ltn_domain)
    }

    pub fn gg_url(&self) -> String {
        format!("https://{}/gg.js", self.ltn_domain)
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
pub async fn search_term(
    fetcher: &dyn Fetcher,
    cfg: &Config,
    version: &str,
    term: &str,
    limit: Option<usize>,
) -> Result<Vec<i32>, SearchError> {
    let key = hash_term(&term.to_lowercase());
    let index_url = cfg.galleries_index_url(version);
    let Some(entry) = b_search(fetcher, &index_url, &key).await? else {
        return Ok(Vec::new());
    };
    let data_url = cfg.galleries_data_url(version);
    fetch_gallery_ids(fetcher, &data_url, entry, limit).await
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
    }
}
