//! 이미지 URL 생성.
//!
//! hitomi는 `gg.js`에 해시 → 경로/서브도메인 매핑 규칙을 담아 서빙하며 주기적
//! 으로 갱신한다. 필요한 것은 두 가지다.
//!
//! - `gg.b` — 경로 접두사 문자열 (예: `'1788595201/'`)
//! - `gg.m(g)` — 큰 switch 문. 나열된 case면 한 값을, 아니면 다른 값을 낸다.
//!
//! **어느 쪽이 어느 값인지는 파일에서 읽는다.** 2026-09-07에 hitomi가 극성을
//! 뒤집었다. 예전에는 `var o = 1`에 나열된 case가 `o = 0`이었는데 지금은
//! `var o = 0`에 나열된 case가 `o = 1`이다. 관찰한 값을 상수로 박아두면 이런
//! 변경에 조용히 틀린 URL을 만들어낸다.
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
    #[error("gg.js format changed: could not read the default value of `o`")]
    MissingDefault,
    #[error("gg.js format changed: could not read the value assigned in the switch")]
    MissingCaseValue,
    #[error("hash is not a 64-character hex string")]
    BadHash,
}

#[derive(Debug, Clone)]
pub struct GgMap {
    pub prefix: String,
    /// switch 문에 나열된 g의 집합.
    pub listed: HashSet<u32>,
    /// 나열되지 않은 g가 받는 값 (`var o = N`).
    pub default_value: u32,
    /// 나열된 g가 받는 값 (`o = M; break;`).
    pub listed_value: u32,
}

impl GgMap {
    pub fn new(
        prefix: String,
        listed: HashSet<u32>,
        default_value: u32,
        listed_value: u32,
    ) -> Self {
        Self { prefix, listed, default_value, listed_value }
    }

    fn m(&self, g: u32) -> u32 {
        if self.listed.contains(&g) { self.listed_value } else { self.default_value }
    }
}

pub fn parse_gg(body: &str) -> Result<GgMap, ImageError> {
    let prefix = extract_prefix(body).ok_or(ImageError::MissingPrefix)?;
    let listed = extract_cases(body);
    if listed.is_empty() {
        return Err(ImageError::NoCases);
    }
    let default_value = extract_number_after(body, "var o =").ok_or(ImageError::MissingDefault)?;
    // switch 안의 대입은 `var o =` 뒤에 나온다. 그 지점 이후에서 찾는다.
    let switch_at = body.find("switch").unwrap_or(0);
    let listed_value =
        extract_number_after(&body[switch_at..], "o =").ok_or(ImageError::MissingCaseValue)?;

    Ok(GgMap { prefix, listed, default_value, listed_value })
}

/// `marker` 바로 뒤에 오는 정수를 읽는다.
fn extract_number_after(body: &str, marker: &str) -> Option<u32> {
    let at = body.find(marker)? + marker.len();
    let rest = body[at..].trim_start();
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
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
        GalleryFile { hash: hash.into(), name: "001.jpg".into(), width: 1, height: 1, hasavif }
    }

    const HASH: &str = "637a35d9d5a892a8b86b97fe9b42e5cf49b8edd6685f7e1baf8f3b361f6cd3e2";

    /// 2026-09-07 이후의 형태: 기본이 0, 나열된 것이 1.
    const CURRENT: &str = "gg = { m: function(g) {\nvar o = 0;\nswitch (g) {\ncase 574:\n\
        case 900:\no = 1; break;\n}\nreturn o;\n},\nb: '999/' };";

    /// 그 이전의 형태: 기본이 1, 나열된 것이 0. 극성이 반대다.
    const LEGACY: &str = "gg = { m: function(g) {\nvar o = 1;\nswitch (g) {\ncase 574:\n\
        case 900:\no = 0; break;\n}\nreturn o;\n},\nb: '999/' };";

    #[test]
    fn parses_real_gg() {
        let gg = parse_gg(include_str!("../tests/fixtures/gg.js")).unwrap();
        assert!(gg.prefix.ends_with('/'));
        assert!(gg.prefix.trim_end_matches('/').chars().all(|c| c.is_ascii_digit()));
        assert!(!gg.listed.is_empty());
        assert_ne!(
            gg.default_value, gg.listed_value,
            "the two branches must differ or the switch would be pointless"
        );
    }

    #[test]
    fn reads_the_polarity_from_the_file() {
        let current = parse_gg(CURRENT).unwrap();
        assert_eq!(current.default_value, 0);
        assert_eq!(current.listed_value, 1);

        // hitomi가 2026-09-07에 이 극성을 뒤집었다. 값을 코드에 박아두면
        // 조용히 틀린 서브도메인을 만들어낸다.
        let legacy = parse_gg(LEGACY).unwrap();
        assert_eq!(legacy.default_value, 1);
        assert_eq!(legacy.listed_value, 0);
    }

    #[test]
    fn the_same_hash_flips_subdomain_when_the_polarity_flips() {
        // 해시 끝이 3e2 -> 세그먼트 574. 두 파일 모두 574를 나열하고 있으므로
        // 극성만으로 서브도메인이 갈린다.
        let current =
            image_url(&Config::default(), &parse_gg(CURRENT).unwrap(), &file(HASH, 1)).unwrap();
        let legacy =
            image_url(&Config::default(), &parse_gg(LEGACY).unwrap(), &file(HASH, 1)).unwrap();
        assert!(current.starts_with("https://a2."), "{current}");
        assert!(legacy.starts_with("https://a1."), "{legacy}");
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
        let gg = GgMap::new("999/".into(), [574u32].into_iter().collect(), 1, 0);
        let url = image_url(&Config::default(), &gg, &file(HASH, 1)).unwrap();
        assert_eq!(url, format!("https://a1.gold-usergeneratedcontent.net/999/574/{HASH}.avif"));
    }

    #[test]
    fn unlisted_segment_uses_the_other_subdomain() {
        let gg = GgMap::new("999/".into(), HashSet::new(), 1, 0);
        let url = image_url(&Config::default(), &gg, &file(HASH, 1)).unwrap();
        assert!(url.starts_with("https://a2."), "{url}");
    }

    #[test]
    fn builds_thumbnail_url_with_tn_subdomain() {
        let gg = GgMap::new("999/".into(), [574u32].into_iter().collect(), 1, 0);
        let url = thumbnail_url(&Config::default(), &gg, HASH).unwrap();
        assert_eq!(
            url,
            format!("https://atn.gold-usergeneratedcontent.net/avifsmallsmalltn/2/3e/{HASH}.avif")
        );
    }

    #[test]
    fn thumbnail_uses_btn_for_unlisted_segment() {
        let gg = GgMap::new("999/".into(), HashSet::new(), 1, 0);
        let url = thumbnail_url(&Config::default(), &gg, HASH).unwrap();
        assert!(url.starts_with("https://btn."), "{url}");
    }

    #[test]
    fn thumbnail_rejects_bad_hash() {
        let gg = GgMap::new("999/".into(), HashSet::new(), 1, 0);
        assert!(thumbnail_url(&Config::default(), &gg, "nope").is_err());
    }

    #[test]
    fn falls_back_to_webp_without_avif() {
        let gg = GgMap::new("999/".into(), HashSet::new(), 1, 0);
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
