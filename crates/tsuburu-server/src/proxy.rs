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
    if ext != "avif" && ext != "webp" {
        return Err(ApiError::bad_request("only avif and webp are proxied"));
    }
    let file = tsuburu_hitomi::GalleryFile {
        hash: hash.to_string(),
        name: String::new(),
        width: 0,
        height: 0,
        hasavif: u8::from(ext == "avif"),
    };

    // A downloaded page is served from disk under the same URL the reader
    // already uses, which is the whole of offline reading.
    if let Some(downloads) = state.downloads.as_ref()
        && let Ok(Some(bytes)) = downloads.read_image(hash, ext)
    {
        return Ok(local_image(bytes, ext));
    }

    let gg = state.gg().await?;
    let url = tsuburu_hitomi::image_url(&state.cfg, &gg, &file)?;

    // A page the reader is about to look at, that nothing has read yet: the
    // bytes are coming down anyway, so hold them long enough to hand over.
    if let Some(waiting) = state.page_to_read(hash).await {
        return read_and_recognise(&state, &url, ext, hash, waiting).await;
    }

    match stream(&state, &url, ext).await {
        Err(err) if err.error == ErrorKind::Network && err.message.contains("404") => {
            // 경로 접두사가 회전했을 가능성이 크다. 한 번만 새로 받아 재시도한다.
            tracing::debug!("image 404; refreshing gg.js and retrying once");
            let gg = state.refresh_gg().await?;
            let url = tsuburu_hitomi::image_url(&state.cfg, &gg, &file)?;
            stream(&state, &url, ext).await
        }
        other => other,
    }
}

pub async fn thumbnail(
    State(state): State<Arc<AppState>>,
    Path(file): Path<String>,
) -> Result<Response, ApiError> {
    let (hash, ext) = split_hash(&file)?;
    if ext != "avif" {
        return Err(ApiError::bad_request("thumbnails are avif only"));
    }
    // 썸네일 경로는 `gg.b`를 쓰지 않으므로 회전의 영향을 받지 않는다.
    let gg = state.gg().await?;
    let url = tsuburu_hitomi::thumbnail_url(&state.cfg, &gg, hash)?;
    stream(&state, &url, ext).await
}

fn split_hash(file: &str) -> Result<(&str, &str), ApiError> {
    let (hash, ext) =
        file.rsplit_once('.').ok_or_else(|| ApiError::bad_request("expected <hash>.<ext>"))?;
    if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ApiError::bad_request("hash must be 64 hex characters"));
    }
    Ok((hash, ext))
}

/// Fetches the page whole, answers with it, and gives a copy to recognition.
///
/// Buffering one page costs a few hundred kilobytes and is the price of not
/// downloading it a second time for the sake of reading its text.
async fn read_and_recognise(
    state: &Arc<AppState>,
    url: &str,
    ext: &str,
    hash: &str,
    waiting: crate::state::UnreadPage,
) -> Result<Response, ApiError> {
    let response = stream(state, url, ext).await?;
    let bytes = match axum::body::to_bytes(response.into_body(), usize::MAX).await {
        Ok(bytes) => bytes.to_vec(),
        Err(err) => {
            return Err(ApiError {
                error: ErrorKind::Network,
                message: format!("could not fetch the image ({err})"),
            });
        }
    };

    state.page_read(hash).await;
    if let Some(grinder) = state.grinder.as_ref() {
        let grinder = Arc::clone(grinder);
        let copy = bytes.clone();
        tokio::spawn(async move {
            grinder
                .recognise_read_page(waiting.gallery, waiting.page, copy, &waiting.language)
                .await;
        });
    }
    Ok(local_image(bytes, ext))
}

fn local_image(bytes: Vec<u8>, ext: &str) -> Response {
    let mut headers = HeaderMap::new();
    if let Ok(value) = HeaderValue::from_str(&format!("image/{ext}")) {
        headers.insert(header::CONTENT_TYPE, value);
    }
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL));
    (StatusCode::OK, headers, bytes).into_response()
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
