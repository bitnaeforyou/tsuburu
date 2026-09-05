//! 이미지 프록시.
//!
//! 브라우저는 hitomi CDN에 직접 붙을 수 없다(Referer 요구, CORS). 서버가
//! 중계하되 **디코드도 리사이즈도 하지 않는다**(스펙 5절). hitomi가 이미
//! avif/webp를 주고 브라우저가 둘 다 읽으므로, 여기서 할 일은 헤더를 붙이고
//! 바이트를 흘리는 것뿐이다. 이미지 처리 crate를 넣지 않는 것만으로 바이너리와
//! 메모리가 크게 줄어든다.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

use crate::error::{ApiError, ErrorKind};
use crate::state::AppState;

/// 프록시한 이미지는 내용이 바뀌지 않는다. 브라우저가 다시 묻지 않게 한다.
const CACHE_CONTROL: &str = "public, max-age=604800, immutable";

pub async fn image(
    State(state): State<Arc<AppState>>,
    Path(file): Path<String>,
) -> Result<Response, ApiError> {
    let (hash, ext) = split_hash(&file)?;
    let gg = state.gg().await?;

    let url = match ext {
        "avif" | "webp" => {
            let dummy = tsuburu_hitomi::GalleryFile {
                hash: hash.to_string(),
                name: String::new(),
                width: 0,
                height: 0,
                hasavif: u8::from(ext == "avif"),
            };
            tsuburu_hitomi::image_url(&state.cfg, &gg, &dummy)?
        }
        _ => return Err(ApiError::bad_request("only avif and webp are proxied")),
    };

    stream(&state, &url, ext).await
}

pub async fn thumbnail(
    State(state): State<Arc<AppState>>,
    Path(file): Path<String>,
) -> Result<Response, ApiError> {
    let (hash, ext) = split_hash(&file)?;
    if ext != "avif" {
        return Err(ApiError::bad_request("thumbnails are avif only"));
    }
    let gg = state.gg().await?;
    let url = tsuburu_hitomi::thumbnail_url(&state.cfg, &gg, hash)?;
    stream(&state, &url, ext).await
}

fn split_hash(file: &str) -> Result<(&str, &str), ApiError> {
    let (hash, ext) = file
        .rsplit_once('.')
        .ok_or_else(|| ApiError::bad_request("expected <hash>.<ext>"))?;
    if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ApiError::bad_request("hash must be 64 hex characters"));
    }
    Ok((hash, ext))
}

async fn stream(state: &AppState, url: &str, ext: &str) -> Result<Response, ApiError> {
    let upstream = state.fetcher.stream(url).await.map_err(|err| ApiError {
        error: ErrorKind::Network,
        message: format!("could not fetch the image ({err})"),
    })?;

    let mut headers = HeaderMap::new();
    if let Ok(value) = HeaderValue::from_str(&format!("image/{ext}")) {
        headers.insert(header::CONTENT_TYPE, value);
    }
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL));
    if let Some(len) = upstream.content_length()
        && let Ok(value) = HeaderValue::from_str(&len.to_string())
    {
        headers.insert(header::CONTENT_LENGTH, value);
    }

    Ok((StatusCode::OK, headers, Body::from_stream(upstream.bytes_stream())).into_response())
}
