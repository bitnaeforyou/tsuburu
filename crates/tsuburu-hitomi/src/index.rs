//! B-tree 탐색과 갤러리 ID 블록 디코딩.

use crate::fetcher::{FetchError, Fetcher};
use crate::node::{DecodeError, Node, decode_node};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;

/// 한 노드를 읽는 데 필요한 고정 바이트 수.
pub const MAX_NODE_SIZE: u64 = 464;

/// 실측 깊이는 6~7이다. 넉넉히 잡되 무한 루프는 막는다.
const MAX_DEPTH: usize = 30;

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error(transparent)]
    Fetch(#[from] FetchError),
    #[error("index format changed: {0}")]
    Decode(#[from] DecodeError),
    #[error("index format changed: block length {got} != expected {want}")]
    BlockLength { got: usize, want: usize },
    #[error("index format changed: implausible gallery count {0}")]
    GalleryCount(i32),
    #[error("index traversal exceeded depth limit")]
    TooDeep,
}

impl SearchError {
    /// 사이트 포맷 변경으로 보이는 오류인가. 네트워크 오류와 구분해 사용자에게
    /// 다른 안내를 내보내기 위한 것이다(스펙 7절).
    pub fn is_format_error(&self) -> bool {
        !matches!(self, SearchError::Fetch(_))
    }
}

/// 검색 키는 검색어의 sha256 앞 4바이트다.
pub fn hash_term(term: &str) -> [u8; 4] {
    let digest = Sha256::digest(term.as_bytes());
    [digest[0], digest[1], digest[2], digest[3]]
}

/// 짧은 쪽 길이까지만 비교한다. hitomi의 `compare_arraybuffers`가 그렇게
/// 동작하며, 길이를 tie-breaker로 쓰면 탐색 결과가 달라진다.
pub fn compare_keys(a: &[u8], b: &[u8]) -> Ordering {
    for (x, y) in a.iter().zip(b.iter()) {
        match x.cmp(y) {
            Ordering::Equal => continue,
            other => return other,
        }
    }
    Ordering::Equal
}

/// (정확히 일치했는가, 위치). 불일치면 내려갈 자식의 인덱스다.
fn locate_key(key: &[u8], node: &Node) -> (bool, usize) {
    for (i, k) in node.keys.iter().enumerate() {
        match compare_keys(key, k) {
            Ordering::Less => return (false, i),
            Ordering::Equal => return (true, i),
            Ordering::Greater => continue,
        }
    }
    (false, node.keys.len())
}

pub async fn b_search(
    fetcher: &dyn Fetcher,
    index_url: &str,
    key: &[u8],
) -> Result<Option<(u64, i32)>, SearchError> {
    let mut address = 0u64;
    for _ in 0..MAX_DEPTH {
        let raw = fetcher.get_range(index_url, address..address + MAX_NODE_SIZE).await?;
        let node = decode_node(&raw)?;
        if node.keys.is_empty() {
            return Ok(None);
        }
        let (found, where_) = locate_key(key, &node);
        if found {
            return Ok(node.datas.get(where_).copied());
        }
        if node.is_leaf() {
            return Ok(None);
        }
        match node.subnode_addresses.get(where_).copied() {
            Some(0) | None => return Ok(None),
            Some(next) => address = next,
        }
    }
    Err(SearchError::TooDeep)
}

pub fn decode_gallery_ids(buf: &[u8]) -> Result<Vec<i32>, SearchError> {
    if buf.len() < 4 {
        return Err(SearchError::BlockLength { got: buf.len(), want: 4 });
    }
    let count = i32::from_be_bytes(buf[0..4].try_into().unwrap());
    if count <= 0 || count > 10_000_000 {
        return Err(SearchError::GalleryCount(count));
    }
    let want = count as usize * 4 + 4;
    if buf.len() != want {
        return Err(SearchError::BlockLength { got: buf.len(), want });
    }
    Ok(read_ids(&buf[4..]))
}

fn read_ids(bytes: &[u8]) -> Vec<i32> {
    bytes.as_chunks::<4>().0.iter().copied().map(i32::from_be_bytes).collect()
}

/// 갤러리 ID 목록을 가져온다.
///
/// `limit`이 주어지면 블록 앞부분만 Range로 읽는다. 결과가 많은 검색어의
/// 데이터 블록은 수백 KB에 달하지만(`school`은 810 KB) 첫 페이지에 필요한
/// 것은 앞의 수십 개뿐이다. 복합 조건은 전체가 필요하므로 `None`을 넘긴다.
pub async fn fetch_gallery_ids(
    fetcher: &dyn Fetcher,
    data_url: &str,
    entry: (u64, i32),
    limit: Option<usize>,
) -> Result<Vec<i32>, SearchError> {
    let (offset, length) = entry;
    if length <= 0 || length > 100_000_000 {
        return Err(SearchError::GalleryCount(length));
    }
    let full = length as u64;
    let want = match limit {
        Some(n) => (4 + n as u64 * 4).min(full),
        None => full,
    };
    let buf = fetcher.get_range(data_url, offset..offset + want).await?;

    if limit.is_none() {
        return decode_gallery_ids(&buf);
    }

    // 부분 읽기에서는 전체 길이 검증을 할 수 없다. 헤더가 말하는 개수와 실제로
    // 받은 바이트 중 작은 쪽까지만 읽는다.
    if buf.len() < 4 {
        return Err(SearchError::BlockLength { got: buf.len(), want: 4 });
    }
    let count = i32::from_be_bytes(buf[0..4].try_into().unwrap());
    if count <= 0 || count > 10_000_000 {
        return Err(SearchError::GalleryCount(count));
    }
    let available = (buf.len() - 4) / 4;
    let take = available.min(count as usize);
    Ok(read_ids(&buf[4..4 + take * 4]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::B;
    use crate::fetcher::mock::MockFetcher;
    use crate::node::write_node;

    #[test]
    fn hash_term_matches_hitomi() {
        // 실제 인덱스 조회로 검증된 값
        assert_eq!(hash_term("naruto"), [0x84, 0x6f, 0x6a, 0x76]);
    }

    #[test]
    fn compare_stops_at_shorter_length() {
        assert_eq!(compare_keys(&[1, 2], &[1, 2, 3]), Ordering::Equal);
        assert_eq!(compare_keys(&[1, 3], &[1, 2, 9]), Ordering::Greater);
        assert_eq!(compare_keys(&[1, 1], &[1, 2, 9]), Ordering::Less);
    }

    #[test]
    fn decodes_gallery_id_block() {
        let mut buf = 2i32.to_be_bytes().to_vec();
        buf.extend_from_slice(&100i32.to_be_bytes());
        buf.extend_from_slice(&200i32.to_be_bytes());
        assert_eq!(decode_gallery_ids(&buf).unwrap(), vec![100, 200]);
    }

    #[test]
    fn rejects_gallery_block_with_wrong_length() {
        let mut buf = 5i32.to_be_bytes().to_vec();
        buf.extend_from_slice(&100i32.to_be_bytes());
        assert!(decode_gallery_ids(&buf).is_err());
    }

    fn two_level_index() -> Vec<u8> {
        let mut index = vec![0u8; 928];
        let (root, child) = index.split_at_mut(464);
        write_node(root, &[&[0x50]], &[(0, 0)], &[464]);
        write_node(child, &[&[0x10]], &[(7, 8)], &[]);
        index
    }

    #[tokio::test]
    async fn b_search_walks_to_child_then_finds_key() {
        let f = MockFetcher::with("idx", two_level_index());
        assert_eq!(b_search(&f, "idx", &[0x10]).await.unwrap(), Some((7, 8)));
        assert_eq!(f.call_count(), 2);
    }

    #[tokio::test]
    async fn b_search_returns_none_for_missing_key_in_leaf() {
        let f = MockFetcher::with("idx", two_level_index());
        assert_eq!(b_search(&f, "idx", &[0x11]).await.unwrap(), None);
    }

    #[tokio::test]
    async fn b_search_reports_format_error_on_garbage() {
        let f = MockFetcher::with("idx", vec![0xff; 464]);
        let err = b_search(&f, "idx", &[0x10]).await.unwrap_err();
        assert!(err.is_format_error());
    }

    #[tokio::test]
    async fn limit_fetches_only_the_prefix() {
        let mut block = 1000i32.to_be_bytes().to_vec();
        for i in 0..1000i32 {
            block.extend_from_slice(&i.to_be_bytes());
        }
        let f = MockFetcher::with("data", block);

        let ids = fetch_gallery_ids(&f, "data", (0, 4004), Some(3)).await.unwrap();
        assert_eq!(ids, vec![0, 1, 2]);

        let (_, range) = f.calls.lock().unwrap()[0].clone();
        assert_eq!(range.end - range.start, 16, "header + 3 ids only");
    }

    #[tokio::test]
    async fn no_limit_fetches_everything_and_validates_length() {
        let mut block = 2i32.to_be_bytes().to_vec();
        block.extend_from_slice(&5i32.to_be_bytes());
        block.extend_from_slice(&6i32.to_be_bytes());
        let f = MockFetcher::with("data", block);
        assert_eq!(fetch_gallery_ids(&f, "data", (0, 12), None).await.unwrap(), vec![5, 6]);
    }

    #[tokio::test]
    async fn limit_larger_than_block_returns_everything() {
        let mut block = 2i32.to_be_bytes().to_vec();
        block.extend_from_slice(&5i32.to_be_bytes());
        block.extend_from_slice(&6i32.to_be_bytes());
        let f = MockFetcher::with("data", block);
        assert_eq!(fetch_gallery_ids(&f, "data", (0, 12), Some(99)).await.unwrap(), vec![5, 6]);
    }

    #[test]
    fn b_is_sixteen() {
        assert_eq!(B, 16);
    }
}
