//! 갤러리 메타데이터 파싱.
//!
//! `GET /galleries/{id}.js`는 `var galleryinfo = {...};` 형태의 자바스크립트
//! 한 줄을 반환한다. 접두사를 떼면 JSON이다.

use serde::{Deserialize, Serialize};

const PREFIX: &str = "var galleryinfo =";

#[derive(Debug, thiserror::Error)]
pub enum GalleryError {
    #[error("gallery format changed: missing `{PREFIX}` prefix")]
    MissingPrefix,
    #[error("gallery format changed: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GalleryFile {
    pub hash: String,
    pub name: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default)]
    pub hasavif: u8,
}

#[derive(Debug, Clone, Deserialize)]
struct RawTag {
    tag: Option<String>,
    #[serde(default)]
    female: serde_json::Value,
    #[serde(default)]
    male: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
struct RawGallery {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    japanese_title: Option<String>,
    #[serde(default)]
    galleryurl: Option<String>,
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    tags: Option<Vec<RawTag>>,
    #[serde(default)]
    files: Vec<GalleryFile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Gallery {
    pub title: Option<String>,
    pub japanese_title: Option<String>,
    pub gallery_url: Option<String>,
    pub kind: Option<String>,
    pub language: Option<String>,
    pub date: Option<String>,
    pub tags: Vec<String>,
    pub files: Vec<GalleryFile>,
}

fn is_truthy(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Number(n) => n.as_i64().is_some_and(|i| i != 0),
        serde_json::Value::String(s) => s != "0" && !s.is_empty(),
        serde_json::Value::Bool(b) => *b,
        _ => false,
    }
}

pub fn parse_gallery_info(body: &str) -> Result<Gallery, GalleryError> {
    let json = body
        .trim_start()
        .strip_prefix(PREFIX)
        .ok_or(GalleryError::MissingPrefix)?
        .trim()
        .trim_end_matches(';');

    let raw: RawGallery = serde_json::from_str(json)?;

    let tags = raw
        .tags
        .unwrap_or_default()
        .into_iter()
        .filter_map(|t| {
            let name = t.tag?;
            Some(if is_truthy(&t.female) {
                format!("female:{name}")
            } else if is_truthy(&t.male) {
                format!("male:{name}")
            } else {
                name
            })
        })
        .collect();

    Ok(Gallery {
        title: raw.title,
        japanese_title: raw.japanese_title,
        gallery_url: raw.galleryurl,
        kind: raw.kind,
        language: raw.language,
        date: raw.date,
        tags,
        files: raw.files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_gallery_info() {
        let body = include_str!("../tests/fixtures/gallery.js");
        let g = parse_gallery_info(body).unwrap();
        assert!(!g.files.is_empty());
        assert_eq!(g.files[0].hash.len(), 64);
        assert!(g.files[0].hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(g.files[0].width > 0 && g.files[0].height > 0);
        assert!(g.gallery_url.is_some());
    }

    #[test]
    fn rejects_body_without_prefix() {
        assert!(matches!(parse_gallery_info(r#"{"files":[]}"#), Err(GalleryError::MissingPrefix)));
    }

    #[test]
    fn reports_json_errors_as_format_changes() {
        assert!(matches!(
            parse_gallery_info("var galleryinfo = {not json};"),
            Err(GalleryError::Json(_))
        ));
    }

    #[test]
    fn namespaces_gendered_tags() {
        let body = r#"var galleryinfo = {"files":[],"tags":[
            {"tag":"glasses","female":"","male":""},
            {"tag":"sole female","female":"1","male":""},
            {"tag":"yaoi","female":"","male":1}]};"#;
        let g = parse_gallery_info(body).unwrap();
        assert_eq!(g.tags, vec!["glasses", "female:sole female", "male:yaoi"]);
    }
}
