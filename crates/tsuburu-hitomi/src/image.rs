//! 이미지 URL 생성.
//!
//! hitomi는 `gg.js`에 해시 → 경로/서브도메인 매핑 규칙을 담아 서빙하며 주기적
//! 으로 갱신한다. 필요한 것은 두 가지다.
//!
//! - `gg.b` — 경로 접두사 문자열 (예: `'1788595201/'`)
//! - `gg.m(g)` — 큰 switch 문. 나열된 case면 0, 아니면 1을 반환한다.
//!
//! 절차는 스펙 부록 A 참고.

use crate::Config;
use crate::gallery::GalleryFile;
use std::collections::HashSet;

/// 실측으로 확인한 썸네일 디렉터리. `webpsmallsmalltn`은 404다.
const THUMBNAIL_DIR: &str = "avifsmallsmalltn";

#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error("gg.js format changed: missing `b` prefix")]
    MissingPrefix,
    #[error("gg.js format changed: no case labels found")]
    NoCases,
    #[error("hash is not a 64-character hex string")]
    BadHash,
}

#[derive(Debug, Clone)]
pub struct GgMap {
    pub prefix: String,
    /// `gg.m(g)`가 0을 반환하는 g의 집합. 그 외에는 1이다.
    pub zeros: HashSet<u32>,
}

impl GgMap {
    pub fn new(prefix: String, zeros: HashSet<u32>) -> Self {
        Self { prefix, zeros }
    }

    fn m(&self, g: u32) -> u32 {
        if self.zeros.contains(&g) { 0 } else { 1 }
    }
}

pub fn parse_gg(body: &str) -> Result<GgMap, ImageError> {
    let prefix = extract_prefix(body).ok_or(ImageError::MissingPrefix)?;
    let zeros = extract_cases(body);
    if zeros.is_empty() {
        return Err(ImageError::NoCases);
    }
    Ok(GgMap { prefix, zeros })
}

/// `b: '1788595201/'` 에서 따옴표 안을 꺼낸다.
fn extract_prefix(body: &str) -> Option<String> {
    let at = body.find("b:")?;
    let rest = &body[at + 2..];
    let open = rest.find('\'')?;
    let after = &rest[open + 1..];
    let close = after.find('\'')?;
    Some(after[..close].to_string())
}

/// `case 3684:` 형태를 모두 모은다. 파일에 switch가 하나뿐이므로 전부 `m`의 것이다.
fn extract_cases(body: &str) -> HashSet<u32> {
    let mut out = HashSet::new();
    for part in body.split("case ").skip(1) {
        let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            continue;
        }
        if !part[digits.len()..].starts_with(':') {
            continue;
        }
        if let Ok(n) = digits.parse::<u32>() {
            out.insert(n);
        }
    }
    out
}

/// 해시 끝 세 글자에서 경로 세그먼트 번호를 얻는다.
///
/// 원본 규칙은 `/(..)(.)$/`로 잘라 `m[2] + m[1]`을 16진수로 읽는 것이다.
/// 예: `...3e2` → `2` + `3e` → `0x23e` → 574.
pub fn segment_from_hash(hash: &str) -> Result<u32, ImageError> {
    if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ImageError::BadHash);
    }
    let tail = &hash[61..];
    let reordered = format!("{}{}", &tail[2..3], &tail[0..2]);
    u32::from_str_radix(&reordered, 16).map_err(|_| ImageError::BadHash)
}

pub fn image_extension(file: &GalleryFile) -> &'static str {
    if file.hasavif != 0 { "avif" } else { "webp" }
}

/// 썸네일 경로는 본문 이미지와 규칙이 다르다. 해시 끝 세 글자를
/// `{끝1}/{앞2}/{해시}`로 펼치고, 서브도메인은 `atn`/`btn`을 쓴다.
///
/// 실측 크기는 3 KB 남짓이라 결과 그리드에 적합하다. 본문 이미지를 그리드에
/// 쓰면 한 화면에 수 MB가 나간다.
pub fn thumbnail_url(cfg: &Config, gg: &GgMap, hash: &str) -> Result<String, ImageError> {
    if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ImageError::BadHash);
    }
    let g = segment_from_hash(hash)?;
    let subdomain = format!("{}tn", (b'a' + gg.m(g) as u8) as char);
    Ok(format!(
        "{}://{}.{}/{}/{}/{}/{}.avif",
        cfg.scheme,
        subdomain,
        cfg.content_domain,
        THUMBNAIL_DIR,
        &hash[63..],
        &hash[61..63],
        hash
    ))
}

pub fn image_url(cfg: &Config, gg: &GgMap, file: &GalleryFile) -> Result<String, ImageError> {
    let g = segment_from_hash(&file.hash)?;
    let subdomain = format!("a{}", 1 + gg.m(g));
    Ok(format!(
        "{}://{}.{}/{}{}/{}.{}",
        cfg.scheme,
        subdomain,
        cfg.content_domain,
        gg.prefix,
        g,
        file.hash,
        image_extension(file)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(hash: &str, hasavif: u8) -> GalleryFile {
        GalleryFile {
            hash: hash.into(),
            name: "001.jpg".into(),
            width: 1,
            height: 1,
            hasavif,
        }
    }

    const HASH: &str = "637a35d9d5a892a8b86b97fe9b42e5cf49b8edd6685f7e1baf8f3b361f6cd3e2";

    #[test]
    fn parses_real_gg() {
        let gg = parse_gg(include_str!("../tests/fixtures/gg.js")).unwrap();
        assert!(gg.prefix.ends_with('/'));
        assert!(gg.prefix.trim_end_matches('/').chars().all(|c| c.is_ascii_digit()));
        assert!(!gg.zeros.is_empty());
    }

    #[test]
    fn segment_reorders_last_three_hex_digits() {
        // 끝이 "3e2" -> "2" + "3e" -> 0x23e -> 574
        assert_eq!(segment_from_hash(HASH).unwrap(), 574);
    }

    #[test]
    fn rejects_malformed_hash() {
        assert!(matches!(segment_from_hash("abc"), Err(ImageError::BadHash)));
        assert!(matches!(segment_from_hash(&"z".repeat(64)), Err(ImageError::BadHash)));
    }

    #[test]
    fn derives_subdomain_and_path_from_hash() {
        let gg = GgMap::new("999/".into(), [574u32].into_iter().collect());
        let url = image_url(&Config::default(), &gg, &file(HASH, 1)).unwrap();
        assert_eq!(
            url,
            format!("https://a1.gold-usergeneratedcontent.net/999/574/{HASH}.avif")
        );
    }

    #[test]
    fn unlisted_segment_uses_the_other_subdomain() {
        let gg = GgMap::new("999/".into(), [1u32].into_iter().collect());
        let url = image_url(&Config::default(), &gg, &file(HASH, 1)).unwrap();
        assert!(url.starts_with("https://a2."), "{url}");
    }

    #[test]
    fn builds_thumbnail_url_with_tn_subdomain() {
        let gg = GgMap::new("999/".into(), [574u32].into_iter().collect());
        let url = thumbnail_url(&Config::default(), &gg, HASH).unwrap();
        assert_eq!(
            url,
            format!("https://atn.gold-usergeneratedcontent.net/avifsmallsmalltn/2/3e/{HASH}.avif")
        );
    }

    #[test]
    fn thumbnail_uses_btn_for_unlisted_segment() {
        let gg = GgMap::new("999/".into(), HashSet::new());
        let url = thumbnail_url(&Config::default(), &gg, HASH).unwrap();
        assert!(url.starts_with("https://btn."), "{url}");
    }

    #[test]
    fn thumbnail_rejects_bad_hash() {
        let gg = GgMap::new("999/".into(), HashSet::new());
        assert!(thumbnail_url(&Config::default(), &gg, "nope").is_err());
    }

    #[test]
    fn falls_back_to_webp_without_avif() {
        let gg = GgMap::new("999/".into(), HashSet::new());
        let url = image_url(&Config::default(), &gg, &file(HASH, 0)).unwrap();
        assert!(url.ends_with(".webp"), "{url}");
    }

    #[test]
    fn missing_prefix_is_a_format_error() {
        assert!(matches!(parse_gg("case 1: o = 0;"), Err(ImageError::MissingPrefix)));
    }

    #[test]
    fn missing_cases_is_a_format_error() {
        assert!(matches!(parse_gg("b: '1/'"), Err(ImageError::NoCases)));
    }
}
