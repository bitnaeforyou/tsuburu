# tsuburu 검색 코어 + 뷰어 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** hitomi 인덱스를 원격에서 직접 읽어 검색하고, 브라우저에서 작품 한 편을 끝까지 볼 수 있는 단일 바이너리를 만든다.

**Architecture:** 카고 워크스페이스. `tsuburu-hitomi`가 hitomi 포맷 지식을 독점하며 `Fetcher` 트레잇에만 의존해 네트워크를 하지 않는다. `tsuburu-fetch`가 reqwest로 그 트레잇을 구현하고 노드 캐시를 얹는다. `tsuburu-server`가 axum으로 JSON API와 이미지 프록시를 제공하고, 빌드된 Svelte 자산을 바이너리에 임베드해 서빙한다.

**Tech Stack:** Rust edition 2024, tokio, axum, reqwest(rustls), serde, quick_cache, rust-embed, clap, tracing / Svelte 5 + Vite + TypeScript

**Spec:** `docs/superpowers/specs/2026-09-05-tsuburu-design.md`

## Global Constraints

- Rust edition 2024. `rust-toolchain.toml`로 툴체인 고정.
- **C 의존성 금지.** reqwest는 `default-features = false`에 `rustls-tls`. OpenSSL, SQLite 등 C 라이브러리를 끌어오는 crate를 추가하지 않는다.
- **hitomi 포맷 지식은 `tsuburu-hitomi` 밖으로 나가지 않는다.** 다른 crate에 바이트 오프셋, 도메인 문자열, URL 규칙이 등장하면 설계 위반이다.
- `tsuburu-hitomi`는 `reqwest`에 의존하지 않는다. `Fetcher` 트레잇만 본다.
- 모든 인덱스 정수는 빅엔디언.
- 상수: `MAX_NODE_SIZE = 464`, `B = 16`, 검색 키 = `sha256(term)[0..4]`.
- 기본 도메인 `ltn.gold-usergeneratedcontent.net`, 이미지 `gold-usergeneratedcontent.net`. 상수가 아니라 설정 구조체 필드로 둔다.
- 이미지 요청에는 `Referer: https://hitomi.la/`를 붙인다.
- 릴리스 프로파일: `lto = "fat"`, `codegen-units = 1`, `opt-level = "z"`, `panic = "abort"`, `strip = true`.
- 네트워크를 타는 테스트에는 `#[ignore]`를 붙인다. `cargo test` 기본 실행은 오프라인에서 통과해야 한다.

---

## 파일 구조

```
Cargo.toml                          워크스페이스 루트
rust-toolchain.toml
crates/
  tsuburu-hitomi/
    src/lib.rs                      공개 API 재수출, Config
    src/fetcher.rs                  Fetcher 트레잇, FetchError
    src/node.rs                     B-tree 노드 디코딩
    src/index.rs                    b_search, 데이터 블록 → 갤러리 ID
    src/search.rs                   질의 파싱, 교집합
    src/gallery.rs                  galleryinfo JSON 파싱
    src/image.rs                    gg.js 파싱, 이미지 URL 생성
    tests/fixtures/                 실제 응답 바이트
  tsuburu-fetch/
    src/lib.rs                      HttpFetcher (reqwest + 캐시)
  tsuburu-server/
    src/lib.rs                      라우터 조립
    src/api.rs                      JSON 핸들러
    src/proxy.rs                    이미지 스트리밍 프록시
    src/assets.rs                   rust-embed 정적 서빙
  tsuburu/
    src/main.rs                     CLI + 서버 기동 + 브라우저 실행
web/
  package.json, vite.config.ts, svelte.config.js
  src/main.ts, src/App.svelte
  src/lib/api.ts
  src/routes/Search.svelte
  src/routes/Gallery.svelte
```

분할 기준은 책임이다. `node.rs`는 바이트를 구조로 바꾸는 것만, `index.rs`는 그 구조 위에서 탐색만 한다. 노드 포맷이 바뀌면 `node.rs`만 고친다.

---

## Task 1: 워크스페이스 골격

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `.gitignore`(수정), `crates/tsuburu-hitomi/Cargo.toml`, `crates/tsuburu-hitomi/src/lib.rs`

**Interfaces:**
- Produces: 워크스페이스 멤버 `tsuburu-hitomi`, `cargo test`가 도는 상태

- [ ] **Step 1: 워크스페이스 루트 작성**

```toml
# Cargo.toml
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
edition = "2024"
license = "MIT"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tracing = "0.1"

[profile.release]
lto = "fat"
codegen-units = 1
opt-level = "z"
panic = "abort"
strip = true
```

- [ ] **Step 2: 툴체인 고정**

```toml
# rust-toolchain.toml
[toolchain]
channel = "1.90"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 3: `cargo check` 통과 확인**

Run: `cargo check --workspace`
Expected: 성공

- [ ] **Step 4: 커밋**

```bash
git add -A && git commit -m "chore: set up cargo workspace"
```

---

## Task 2: Fetcher 트레잇

**Files:**
- Create: `crates/tsuburu-hitomi/src/fetcher.rs`
- Modify: `crates/tsuburu-hitomi/src/lib.rs`
- Test: `crates/tsuburu-hitomi/src/fetcher.rs` 내 `#[cfg(test)]`

**Interfaces:**
- Produces: `trait Fetcher`, `FetchError`, 테스트용 `MockFetcher`

`tsuburu-hitomi`가 네트워크를 하지 않게 만드는 경계다. 이후 모든 파싱 테스트가 이 목 위에서 돈다.

- [ ] **Step 1: 트레잇 작성**

```rust
// fetcher.rs
use std::ops::Range;

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("network error: {0}")]
    Network(String),
    #[error("unexpected status {0}")]
    Status(u16),
}

pub trait Fetcher: Send + Sync {
    fn get_range(
        &self,
        url: &str,
        range: Range<u64>,
    ) -> impl Future<Output = Result<Vec<u8>, FetchError>> + Send;

    fn get(&self, url: &str) -> impl Future<Output = Result<Vec<u8>, FetchError>> + Send;
}
```

- [ ] **Step 2: 테스트용 목 작성**

```rust
#[cfg(any(test, feature = "mock"))]
pub mod mock {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    pub struct MockFetcher {
        pub bodies: HashMap<String, Vec<u8>>,
        pub calls: std::sync::Mutex<Vec<(String, Range<u64>)>>,
    }

    impl MockFetcher {
        pub fn with(url: &str, body: Vec<u8>) -> Self {
            let mut bodies = HashMap::new();
            bodies.insert(url.to_string(), body);
            Self { bodies, calls: Default::default() }
        }
    }

    impl Fetcher for MockFetcher {
        async fn get_range(&self, url: &str, range: Range<u64>) -> Result<Vec<u8>, FetchError> {
            self.calls.lock().unwrap().push((url.to_string(), range.clone()));
            let body = self.bodies.get(url).ok_or(FetchError::Status(404))?;
            let start = range.start as usize;
            let end = (range.end as usize).min(body.len());
            Ok(body[start.min(body.len())..end].to_vec())
        }

        async fn get(&self, url: &str) -> Result<Vec<u8>, FetchError> {
            self.bodies.get(url).cloned().ok_or(FetchError::Status(404))
        }
    }
}
```

- [ ] **Step 3: 목 동작 테스트**

```rust
#[tokio::test]
async fn mock_returns_requested_range() {
    let f = mock::MockFetcher::with("u", (0u8..10).collect());
    assert_eq!(f.get_range("u", 2..5).await.unwrap(), vec![2, 3, 4]);
}
```

Run: `cargo test -p tsuburu-hitomi`
Expected: PASS

- [ ] **Step 4: 커밋**

---

## Task 3: 노드 디코딩

**Files:**
- Create: `crates/tsuburu-hitomi/src/node.rs`, `crates/tsuburu-hitomi/tests/fixtures/root_node.bin`
- Test: `node.rs` 내 `#[cfg(test)]`

**Interfaces:**
- Consumes: 없음 (순수 함수)
- Produces: `pub struct Node { keys: Vec<Vec<u8>>, datas: Vec<(u64, i32)>, subnode_addresses: [u64; 17] }`, `pub fn decode_node(data: &[u8]) -> Result<Node, DecodeError>`, `Node::is_leaf(&self) -> bool`

포맷은 스펙 부록 A를 따른다. 464바이트, 빅엔디언.

- [ ] **Step 1: 픽스처 저장**

실제 루트 노드를 받아 파일로 둔다. 이후 테스트는 네트워크 없이 돈다.

```bash
V=$(curl -s "https://ltn.gold-usergeneratedcontent.net/galleriesindex/version?_=$(date +%s)")
curl -s -H "Range: bytes=0-463" \
  "https://ltn.gold-usergeneratedcontent.net/galleriesindex/galleries.$V.index" \
  -o crates/tsuburu-hitomi/tests/fixtures/root_node.bin
```

- [ ] **Step 2: 실패하는 테스트 작성**

```rust
#[test]
fn decodes_real_root_node() {
    let data = include_bytes!("../tests/fixtures/root_node.bin");
    let node = decode_node(data).unwrap();
    assert!(!node.keys.is_empty());
    assert!(node.keys.iter().all(|k| !k.is_empty() && k.len() <= 32));
    assert_eq!(node.keys.len(), node.datas.len());
    assert!(!node.is_leaf(), "root of a large index must have children");
}

#[test]
fn rejects_absurd_key_size() {
    let mut data = vec![0u8; 464];
    data[0..4].copy_from_slice(&1i32.to_be_bytes());
    data[4..8].copy_from_slice(&99i32.to_be_bytes());
    assert!(matches!(decode_node(&data), Err(DecodeError::KeySize(99))));
}

#[test]
fn rejects_truncated_buffer() {
    assert!(decode_node(&[0u8; 3]).is_err());
}
```

- [ ] **Step 3: 실패 확인**

Run: `cargo test -p tsuburu-hitomi node`
Expected: FAIL — `decode_node` 미정의

- [ ] **Step 4: 구현**

```rust
use crate::B;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum DecodeError {
    #[error("buffer too short at offset {0}")]
    Truncated(usize),
    #[error("invalid key size {0}")]
    KeySize(i32),
    #[error("invalid count {0}")]
    Count(i32),
}

#[derive(Debug, Clone)]
pub struct Node {
    pub keys: Vec<Vec<u8>>,
    pub datas: Vec<(u64, i32)>,
    pub subnode_addresses: [u64; B + 1],
}

impl Node {
    pub fn is_leaf(&self) -> bool {
        self.subnode_addresses.iter().all(|&a| a == 0)
    }
}

struct Cursor<'a> { data: &'a [u8], pos: usize }

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let end = self.pos.checked_add(n).ok_or(DecodeError::Truncated(self.pos))?;
        let s = self.data.get(self.pos..end).ok_or(DecodeError::Truncated(self.pos))?;
        self.pos = end;
        Ok(s)
    }
    fn i32(&mut self) -> Result<i32, DecodeError> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, DecodeError> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
}

pub fn decode_node(data: &[u8]) -> Result<Node, DecodeError> {
    let mut c = Cursor { data, pos: 0 };

    let n_keys = c.i32()?;
    if !(0..=B as i32).contains(&n_keys) {
        return Err(DecodeError::Count(n_keys));
    }
    let mut keys = Vec::with_capacity(n_keys as usize);
    for _ in 0..n_keys {
        let size = c.i32()?;
        if size <= 0 || size > 32 {
            return Err(DecodeError::KeySize(size));
        }
        keys.push(c.take(size as usize)?.to_vec());
    }

    let n_datas = c.i32()?;
    if !(0..=B as i32).contains(&n_datas) {
        return Err(DecodeError::Count(n_datas));
    }
    let mut datas = Vec::with_capacity(n_datas as usize);
    for _ in 0..n_datas {
        let offset = c.u64()?;
        let length = c.i32()?;
        datas.push((offset, length));
    }

    let mut subnode_addresses = [0u64; B + 1];
    for slot in subnode_addresses.iter_mut() {
        *slot = c.u64()?;
    }

    Ok(Node { keys, datas, subnode_addresses })
}
```

- [ ] **Step 5: 통과 확인**

Run: `cargo test -p tsuburu-hitomi node`
Expected: PASS

- [ ] **Step 6: 커밋**

---

## Task 4: 키 비교와 B-tree 탐색

**Files:**
- Create: `crates/tsuburu-hitomi/src/index.rs`
- Test: `index.rs` 내 `#[cfg(test)]`

**Interfaces:**
- Consumes: `Node`, `decode_node`, `Fetcher`
- Produces: `pub fn hash_term(term: &str) -> [u8; 4]`, `pub async fn b_search<F: Fetcher>(f: &F, index_url: &str, key: &[u8]) -> Result<Option<(u64, i32)>, SearchError>`, `pub fn decode_gallery_ids(buf: &[u8]) -> Result<Vec<i32>, SearchError>`

**주의:** 키 비교는 짧은 쪽 길이까지만 본다. 원본 JS의 `compare_arraybuffers`가 그렇게 동작하며, 이를 따르지 않으면 결과가 달라진다.

- [ ] **Step 1: 실패하는 테스트 작성**

```rust
#[test]
fn hash_term_matches_hitomi() {
    // sha256("naruto")[0..4] — 실제 인덱스로 검증된 값
    assert_eq!(hash_term("naruto"), [0x84, 0x6f, 0x6a, 0x76]);
}

#[test]
fn compare_stops_at_shorter_length() {
    assert_eq!(compare_keys(&[1, 2], &[1, 2, 3]), std::cmp::Ordering::Equal);
    assert_eq!(compare_keys(&[1, 3], &[1, 2, 9]), std::cmp::Ordering::Greater);
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

#[tokio::test]
async fn b_search_walks_to_child_then_finds_key() {
    // 루트: 키 [0x50], 자식 주소 [464, 0, ...] — 검색 키 [0x10]은 첫 자식으로
    // 자식: 키 [0x10] 일치, data (7, 8)
    let mut index = vec![0u8; 928];
    write_node(&mut index[0..464], &[&[0x50]], &[(0, 0)], &[464]);
    write_node(&mut index[464..928], &[&[0x10]], &[(7, 8)], &[]);
    let f = mock::MockFetcher::with("idx", index);
    let hit = b_search(&f, "idx", &[0x10]).await.unwrap();
    assert_eq!(hit, Some((7, 8)));
    assert_eq!(f.calls.lock().unwrap().len(), 2);
}
```

`write_node`는 테스트 헬퍼다. 노드 포맷대로 바이트를 채운다.

```rust
#[cfg(test)]
fn write_node(out: &mut [u8], keys: &[&[u8]], datas: &[(u64, i32)], subs: &[u64]) {
    let mut p = 0;
    let mut put = |b: &[u8], p: &mut usize| { out[*p..*p + b.len()].copy_from_slice(b); *p += b.len(); };
    put(&(keys.len() as i32).to_be_bytes(), &mut p);
    for k in keys {
        put(&(k.len() as i32).to_be_bytes(), &mut p);
        put(k, &mut p);
    }
    put(&(datas.len() as i32).to_be_bytes(), &mut p);
    for (o, l) in datas {
        put(&o.to_be_bytes(), &mut p);
        put(&l.to_be_bytes(), &mut p);
    }
    for i in 0..(B + 1) {
        put(&subs.get(i).copied().unwrap_or(0).to_be_bytes(), &mut p);
    }
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p tsuburu-hitomi index`
Expected: FAIL

- [ ] **Step 3: 구현**

```rust
use crate::fetcher::{FetchError, Fetcher};
use crate::node::{decode_node, DecodeError, Node};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;

pub const MAX_NODE_SIZE: u64 = 464;
const MAX_DEPTH: usize = 30;

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error(transparent)]
    Fetch(#[from] FetchError),
    #[error("index format changed: {0}")]
    Decode(#[from] DecodeError),
    #[error("index format changed: gallery block length {got} != expected {want}")]
    BlockLength { got: usize, want: usize },
    #[error("index format changed: implausible gallery count {0}")]
    GalleryCount(i32),
    #[error("index traversal exceeded depth limit")]
    TooDeep,
}

pub fn hash_term(term: &str) -> [u8; 4] {
    let digest = Sha256::digest(term.as_bytes());
    [digest[0], digest[1], digest[2], digest[3]]
}

pub fn compare_keys(a: &[u8], b: &[u8]) -> Ordering {
    for (x, y) in a.iter().zip(b.iter()) {
        match x.cmp(y) {
            Ordering::Equal => continue,
            other => return other,
        }
    }
    Ordering::Equal
}

/// 노드 안에서 키 위치를 찾는다. (정확히 일치했는가, 인덱스)
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

pub async fn b_search<F: Fetcher>(
    fetcher: &F,
    index_url: &str,
    key: &[u8],
) -> Result<Option<(u64, i32)>, SearchError> {
    let mut address = 0u64;
    for _ in 0..MAX_DEPTH {
        let raw = fetcher
            .get_range(index_url, address..address + MAX_NODE_SIZE)
            .await?;
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
        let next = node.subnode_addresses[where_];
        if next == 0 {
            return Ok(None);
        }
        address = next;
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
    Ok(buf[4..]
        .chunks_exact(4)
        .map(|c| i32::from_be_bytes(c.try_into().unwrap()))
        .collect())
}
```

`sha2`를 `tsuburu-hitomi`의 의존성에 추가한다(순수 Rust).

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p tsuburu-hitomi index`
Expected: PASS

- [ ] **Step 5: 커밋**

---

## Task 5: 부분 데이터 블록 읽기

**Files:**
- Modify: `crates/tsuburu-hitomi/src/index.rs`

**Interfaces:**
- Produces: `pub async fn fetch_gallery_ids<F: Fetcher>(f: &F, data_url: &str, entry: (u64, i32), limit: Option<usize>) -> Result<Vec<i32>, SearchError>`

스펙 12절에서 확인한 최적화다. `school`의 데이터 블록은 810 KB지만 첫 페이지에는 앞의 수십 개만 필요하다. `limit`이 주어지면 `4 + limit*4` 바이트만 Range로 가져온다.

- [ ] **Step 1: 실패하는 테스트 작성**

```rust
#[tokio::test]
async fn limit_fetches_only_the_prefix() {
    let mut block = 1000i32.to_be_bytes().to_vec();
    for i in 0..1000i32 { block.extend_from_slice(&i.to_be_bytes()); }
    let f = mock::MockFetcher::with("data", block);

    let ids = fetch_gallery_ids(&f, "data", (0, 4004), Some(3)).await.unwrap();
    assert_eq!(ids, vec![0, 1, 2]);

    let (_, range) = f.calls.lock().unwrap()[0].clone();
    assert_eq!(range.end - range.start, 16, "must request only header + 3 ids");
}

#[tokio::test]
async fn no_limit_fetches_everything_and_validates_length() {
    let mut block = 2i32.to_be_bytes().to_vec();
    block.extend_from_slice(&5i32.to_be_bytes());
    block.extend_from_slice(&6i32.to_be_bytes());
    let f = mock::MockFetcher::with("data", block);
    assert_eq!(fetch_gallery_ids(&f, "data", (0, 12), None).await.unwrap(), vec![5, 6]);
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p tsuburu-hitomi fetch_gallery_ids`
Expected: FAIL

- [ ] **Step 3: 구현**

```rust
pub async fn fetch_gallery_ids<F: Fetcher>(
    fetcher: &F,
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
    // 부분 읽기에서는 전체 길이 검증을 할 수 없다. 헤더가 말하는 개수와
    // 실제로 받은 바이트 중 작은 쪽까지만 읽는다.
    if buf.len() < 4 {
        return Err(SearchError::BlockLength { got: buf.len(), want: 4 });
    }
    let count = i32::from_be_bytes(buf[0..4].try_into().unwrap());
    if count <= 0 || count > 10_000_000 {
        return Err(SearchError::GalleryCount(count));
    }
    let available = (buf.len() - 4) / 4;
    let take = available.min(count as usize);
    Ok(buf[4..4 + take * 4]
        .chunks_exact(4)
        .map(|c| i32::from_be_bytes(c.try_into().unwrap()))
        .collect())
}
```

- [ ] **Step 4: 통과 확인 후 커밋**

---

## Task 6: 질의 파싱과 교집합

**Files:**
- Create: `crates/tsuburu-hitomi/src/search.rs`

**Interfaces:**
- Consumes: `hash_term`, `b_search`, `fetch_gallery_ids`
- Produces: `pub struct Query { pub include: Vec<String>, pub exclude: Vec<String> }`, `pub fn parse_query(s: &str) -> Query`, `pub fn intersect(a: &[i32], b: &[i32]) -> Vec<i32>`, `pub fn difference(a: &[i32], b: &[i32]) -> Vec<i32>`

갤로핑 교집합으로 구현한다. 스펙 12절에서 병목이 아님이 확인됐으므로 SIMD로 올리지 않는다.

**주의:** 갤러리 ID 목록은 최신순으로 **내림차순** 정렬돼 있다(측정 결과 `[4170513, 4169752, ...]`). 오름차순을 가정하면 교집합이 빈 결과를 낸다.

- [ ] **Step 1: 실패하는 테스트 작성**

```rust
#[test]
fn parses_terms_and_negations() {
    let q = parse_query("  school -yaoi Glasses ");
    assert_eq!(q.include, vec!["school", "glasses"]);
    assert_eq!(q.exclude, vec!["yaoi"]);
}

#[test]
fn intersects_descending_lists() {
    assert_eq!(intersect(&[9, 7, 5, 3, 1], &[8, 7, 3, 2]), vec![7, 3]);
}

#[test]
fn intersection_of_disjoint_is_empty() {
    assert!(intersect(&[9, 7], &[8, 6]).is_empty());
}

#[test]
fn difference_removes_excluded() {
    assert_eq!(difference(&[9, 7, 5, 3], &[7, 3]), vec![9, 5]);
}

#[test]
fn galloping_handles_very_lopsided_inputs() {
    let big: Vec<i32> = (0..10_000).rev().collect();
    assert_eq!(intersect(&big, &[5000, 17]), vec![5000, 17]);
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p tsuburu-hitomi search`
Expected: FAIL

- [ ] **Step 3: 구현**

```rust
#[derive(Debug, Default, PartialEq)]
pub struct Query {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

pub fn parse_query(s: &str) -> Query {
    let mut q = Query::default();
    for token in s.split_whitespace() {
        match token.strip_prefix('-') {
            Some(rest) if !rest.is_empty() => q.exclude.push(rest.to_lowercase()),
            _ => q.include.push(token.to_lowercase()),
        }
    }
    q
}

/// 내림차순 정렬된 두 목록의 교집합. 갤로핑(지수) 탐색으로 건너뛴다.
pub fn intersect(a: &[i32], b: &[i32]) -> Vec<i32> {
    let (small, large) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let mut out = Vec::with_capacity(small.len());
    let mut j = 0usize;
    for &v in small {
        // 내림차순이므로 large[j] > v 인 동안 전진한다.
        let mut step = 1usize;
        while j + step < large.len() && large[j + step] > v {
            j += step;
            step *= 2;
        }
        let hi = (j + step).min(large.len());
        // [j, hi) 구간에서 v의 위치를 이진 탐색
        let slice = &large[j..hi];
        match slice.binary_search_by(|probe| v.cmp(probe)) {
            Ok(k) => {
                out.push(v);
                j += k;
            }
            Err(k) => j += k,
        }
        if j >= large.len() {
            break;
        }
    }
    out
}

pub fn difference(a: &[i32], b: &[i32]) -> Vec<i32> {
    let excluded: std::collections::HashSet<i32> = b.iter().copied().collect();
    a.iter().copied().filter(|v| !excluded.contains(v)).collect()
}
```

- [ ] **Step 4: 통과 확인 후 커밋**

---

## Task 7: 실제 인덱스에 대한 통합 테스트

**Files:**
- Create: `crates/tsuburu-hitomi/tests/live.rs`

네트워크를 타므로 `#[ignore]`를 붙인다. `cargo test -- --ignored`로만 돈다.

- [ ] **Step 1: 테스트 작성**

```rust
// tests/live.rs
// 실제 hitomi에 붙는다. `cargo test -p tsuburu-hitomi -- --ignored`로 실행.
#[tokio::test]
#[ignore = "requires network access to hitomi"]
async fn searches_real_index_for_naruto() {
    let fetcher = tsuburu_fetch::HttpFetcher::new(Default::default());
    let cfg = tsuburu_hitomi::Config::default();
    let version = tsuburu_hitomi::galleries_index_version(&fetcher, &cfg).await.unwrap();
    let ids = tsuburu_hitomi::search_term(&fetcher, &cfg, &version, "naruto", Some(10))
        .await
        .unwrap();
    assert!(!ids.is_empty(), "naruto must match galleries");
    assert!(ids.len() <= 10);
    assert!(ids.windows(2).all(|w| w[0] > w[1]), "ids are descending");
}
```

- [ ] **Step 2: 실행해 통과 확인**

Run: `cargo test -p tsuburu-hitomi -- --ignored`
Expected: PASS

- [ ] **Step 3: 커밋**

---

## Task 8: Config와 상위 API

**Files:**
- Modify: `crates/tsuburu-hitomi/src/lib.rs`

**Interfaces:**
- Produces: `pub struct Config { pub ltn_domain: String, pub content_domain: String }` (`Default`는 `ltn.gold-usergeneratedcontent.net` / `gold-usergeneratedcontent.net`), `pub async fn galleries_index_version<F: Fetcher>(f, cfg) -> Result<String, SearchError>`, `pub async fn search_term<F: Fetcher>(f, cfg, version, term, limit) -> Result<Vec<i32>, SearchError>`, `pub async fn search<F: Fetcher>(f, cfg, version, query: &Query) -> Result<Vec<i32>, SearchError>`

URL 조립은 전부 여기 모은다. 다른 crate가 URL을 만들면 설계 위반이다.

- [ ] **Step 1: 테스트 작성**

```rust
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
}
```

- [ ] **Step 2: 구현**

`search`는 include 항목을 각각 조회해 교집합을 취하고, exclude를 차집합으로 뺀다. include가 여럿이면 전체 목록이 필요하므로 `limit`을 쓰지 않는다. include가 하나면 `limit`을 넘겨 부분 읽기를 활용한다.

- [ ] **Step 3: 통과 확인 후 커밋**

---

## Task 9: HttpFetcher와 노드 캐시

**Files:**
- Create: `crates/tsuburu-fetch/Cargo.toml`, `crates/tsuburu-fetch/src/lib.rs`

**Interfaces:**
- Consumes: `tsuburu_hitomi::Fetcher`
- Produces: `pub struct HttpFetcher`, `HttpFetcher::new(FetchConfig) -> Self`, `pub struct FetchConfig { pub max_concurrent: usize, pub cache_entries: usize, pub user_agent: String }`

스펙 12절이 지목한 최적화가 여기 들어간다. 노드 요청(464바이트)은 `(url, range.start)`를 키로 캐시한다. 상위 노드는 모든 검색에서 재사용되므로 적중률이 높다.

- [ ] **Step 1: 캐시 테스트 작성**

```rust
#[tokio::test]
async fn repeated_node_request_hits_cache() {
    // 같은 (url, offset) 두 번 요청 시 내부 HTTP 호출은 한 번
    // wiremock으로 요청 수를 센다
}
```

`wiremock`을 dev-dependency로 추가한다.

- [ ] **Step 2: 구현**

reqwest 클라이언트는 `default-features = false`, `features = ["rustls-tls", "http2", "stream"]`. `Range` 헤더를 붙이고 200/206만 허용한다. 동시 요청은 `Semaphore`로 제한한다. 캐시는 `quick_cache`.

- [ ] **Step 3: 통과 확인 후 커밋**

---

## Task 10: CLI (마일스톤 1 완료)

**Files:**
- Create: `crates/tsuburu/Cargo.toml`, `crates/tsuburu/src/main.rs`

**Interfaces:**
- Produces: `tsuburu search <query> [--limit N]`

- [ ] **Step 1: clap으로 CLI 정의, 검색 실행, ID 출력**

- [ ] **Step 2: 실제로 돌려 확인**

Run: `cargo run -p tsuburu -- search naruto --limit 10`
Expected: 갤러리 ID 10개 출력, 왕복 횟수와 소요 시간을 stderr에 로그

- [ ] **Step 3: 커밋**

---

## Task 11: 갤러리 메타데이터

**Files:**
- Create: `crates/tsuburu-hitomi/src/gallery.rs`, `crates/tsuburu-hitomi/tests/fixtures/gallery.js`

**Interfaces:**
- Produces: `pub struct Gallery { pub id: i32, pub title: Option<String>, pub gallery_url: String, pub kind: String, pub files: Vec<GalleryFile> }`, `pub struct GalleryFile { pub hash: String, pub name: String, pub width: u32, pub height: u32, pub hasavif: u8 }`, `pub fn parse_gallery_info(body: &str) -> Result<Gallery, GalleryError>`

`GET /galleries/{id}.js`는 `var galleryinfo = {...};`를 반환한다. 접두사를 떼고 JSON으로 파싱한다.

- [ ] **Step 1: 픽스처 저장**

```bash
curl -s "https://ltn.gold-usergeneratedcontent.net/galleries/4170513.js" \
  -o crates/tsuburu-hitomi/tests/fixtures/gallery.js
```

- [ ] **Step 2: 실패하는 테스트 작성**

```rust
#[test]
fn parses_real_gallery_info() {
    let body = include_str!("../tests/fixtures/gallery.js");
    let g = parse_gallery_info(body).unwrap();
    assert!(!g.files.is_empty());
    assert_eq!(g.files[0].hash.len(), 64);
    assert!(g.files[0].width > 0 && g.files[0].height > 0);
}

#[test]
fn rejects_body_without_prefix() {
    assert!(parse_gallery_info("{\"files\":[]}").is_err());
}
```

- [ ] **Step 3: 구현 후 통과 확인, 커밋**

---

## Task 12: 이미지 URL 생성 (gg.js)

**Files:**
- Create: `crates/tsuburu-hitomi/src/image.rs`, `crates/tsuburu-hitomi/tests/fixtures/gg.js`

**Interfaces:**
- Produces: `pub struct GgMap { prefix: String, zeros: HashSet<u32> }`, `pub fn parse_gg(body: &str) -> Result<GgMap, ImageError>`, `pub fn image_url(cfg: &Config, gg: &GgMap, file: &GalleryFile) -> String`

규칙은 스펙 부록 A에 있다. `gg.b`는 경로 접두사, `gg.m(g)`는 나열된 case면 0, 아니면 1.

- [ ] **Step 1: 픽스처 저장**

```bash
curl -s "https://ltn.gold-usergeneratedcontent.net/gg.js" \
  -o crates/tsuburu-hitomi/tests/fixtures/gg.js
```

- [ ] **Step 2: 실패하는 테스트 작성**

```rust
#[test]
fn parses_real_gg() {
    let gg = parse_gg(include_str!("../tests/fixtures/gg.js")).unwrap();
    assert!(gg.prefix.ends_with('/'));
    assert!(gg.prefix.trim_end_matches('/').chars().all(|c| c.is_ascii_digit()));
    assert!(!gg.zeros.is_empty());
}

#[test]
fn derives_subdomain_and_path_from_hash() {
    let gg = GgMap { prefix: "999/".into(), zeros: [0x23e].into_iter().collect() };
    let file = GalleryFile {
        hash: "637a35d9d5a892a8b86b97fe9b42e5cf49b8edd6685f7e1baf8f3b361f6cd3e2".into(),
        name: "001.jpg".into(), width: 1, height: 1, hasavif: 1,
    };
    // 끝 3글자 "3e2" -> g = 0x23e, zeros에 있으므로 m=0 -> 서브도메인 a1
    let url = image_url(&Config::default(), &gg, &file);
    assert!(url.starts_with("https://a1.gold-usergeneratedcontent.net/999/574/"), "{url}");
    assert!(url.ends_with(".avif"));
}

#[test]
fn falls_back_to_webp_without_avif() {
    // hasavif = 0 이면 확장자가 webp
}
```

`0x23e = 574`이므로 경로 세그먼트는 `574`다.

- [ ] **Step 3: 구현**

`gg.b`는 정규식 `b:\s*'([^']+)'`로, case 집합은 `case\s+(\d+):`를 모두 모아서 얻는다. switch 안의 case만 모으면 되고 다른 case 문은 파일에 없다.

- [ ] **Step 4: 통과 확인 후 커밋**

---

## Task 13: 서버 — API와 이미지 프록시

**Files:**
- Create: `crates/tsuburu-server/Cargo.toml`, `src/lib.rs`, `src/api.rs`, `src/proxy.rs`, `src/assets.rs`

**Interfaces:**
- Produces: `pub fn router(state: AppState) -> axum::Router`
- API:
  - `GET /api/search?q=<query>&offset=<n>&limit=<n>` → `{ "total": n, "ids": [..] }`
  - `GET /api/gallery/{id}` → `Gallery` + 각 파일의 프록시 URL
  - `GET /img/{hash}.{ext}` → 이미지 바이트 스트리밍

- [ ] **Step 1: API 계약 테스트 작성**

`MockFetcher`를 주입한 상태로 라우터를 만들고 `tower::ServiceExt::oneshot`으로 호출해 응답 형태를 검증한다.

```rust
#[tokio::test]
async fn search_endpoint_returns_ids() {
    let app = router(test_state_with_fixture());
    let res = app.oneshot(Request::builder().uri("/api/search?q=naruto").body(Body::empty()).unwrap())
        .await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    // 본문에 ids 배열이 있다
}

#[tokio::test]
async fn unknown_index_version_returns_clear_error() {
    // 포맷 오류는 502가 아니라 명시적 코드와 메시지로 나온다
    // { "error": "format_changed", "message": "..." }
}
```

- [ ] **Step 2: 구현**

에러는 네트워크(`network`)와 포맷(`format_changed`)을 구분해 JSON으로 내린다. 스펙 7절의 요구다.

이미지 프록시는 `Referer: https://hitomi.la/`를 붙이고 응답 바이트를 그대로 스트리밍한다. 디코드하지 않는다(스펙 5절).

- [ ] **Step 3: 통과 확인 후 커밋**

---

## Task 14: 프론트엔드

**Files:**
- Create: `web/package.json`, `web/vite.config.ts`, `web/svelte.config.js`, `web/tsconfig.json`, `web/index.html`, `web/src/main.ts`, `web/src/App.svelte`, `web/src/lib/api.ts`, `web/src/routes/Search.svelte`, `web/src/routes/Gallery.svelte`

- [ ] **Step 1: Vite + Svelte 5 프로젝트 구성**

빌드 출력은 `web/dist`. dev 서버는 `/api`와 `/img`를 `http://127.0.0.1:8420`으로 프록시한다.

- [ ] **Step 2: 검색 화면**

검색창, 결과 그리드. 그리드는 `content-visibility: auto`와 `contain-intrinsic-size`로 오프스크린 렌더링을 건너뛴다. 라이브러리를 쓰지 않는다.

- [ ] **Step 3: 갤러리 뷰어**

세로 스크롤 뷰어. 좌우 방향키로 페이지 이동, 다음 2장을 `img.decode()`로 미리 디코드한다.

- [ ] **Step 4: 해시 라우터**

`#/`(검색), `#/g/{id}`(갤러리). 30줄 이내.

- [ ] **Step 5: 빌드 확인 후 커밋**

Run: `cd web && npm install && npm run build`
Expected: `web/dist` 생성

---

## Task 15: 자산 임베드와 바이너리 기동

**Files:**
- Modify: `crates/tsuburu-server/src/assets.rs`, `crates/tsuburu/src/main.rs`

- [ ] **Step 1: rust-embed로 `web/dist` 임베드**

`web/dist`가 없을 때도 `cargo check`가 실패하지 않도록 디렉터리를 커밋해두거나 `debug-embed`를 끈 상태를 처리한다.

- [ ] **Step 2: `tsuburu` 실행 시 포트 확보 → 서버 기동 → 브라우저 열기**

빈 포트를 찾아 바인딩하고 `open` crate로 브라우저를 연다. `--no-open`, `--port` 플래그를 둔다.

- [ ] **Step 3: 손으로 확인**

Run: `cargo run -p tsuburu`
Expected: 브라우저가 열리고, 검색해서 갤러리를 열어 이미지가 보인다

- [ ] **Step 4: 커밋**

---

## Self-Review 결과

**스펙 커버리지:** 3절 모듈 경계 → Task 1~13. 4절 검색 → Task 3~6, 8. 5절 이미지 → Task 12~13. 6절 스택 → 전 태스크의 Global Constraints. 7절 실패 설계 → Task 4(포맷 오류 타입), Task 13(오류 구분 응답). 8절 테스트 → 각 태스크의 테스트 단계, Task 7(ignore된 통합 테스트). 12절 성능 → Task 5(부분 읽기), Task 9(노드 캐시).

**미포함(별도 계획):** 마일스톤 4(즐겨찾기·기록·다운로드, `tsuburu-store`), 마일스톤 5(릴리스 CI, 연령 고지). 이 계획만으로 동작하는 소프트웨어가 나오므로 분리한다.

**타입 일관성:** `Fetcher`(Task 2) → `b_search`(Task 4) → `fetch_gallery_ids`(Task 5) → `search`(Task 8) → `HttpFetcher`(Task 9) → 서버(Task 13). `GalleryFile`은 Task 11에서 정의하고 Task 12에서 사용한다. `Config`는 Task 8에서 정의하고 Task 12~13에서 사용한다.
