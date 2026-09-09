//! 오류를 사용자가 취할 조치별로 구분해 내려보낸다(스펙 7절).
//!
//! 사이트 포맷이 바뀐 것과 네트워크가 안 되는 것은 사용자가 할 일이 다르다.
//! 전자는 업데이트를 기다려야 하고 후자는 연결을 확인하면 된다. 뭉뚱그려
//! 500을 내면 둘 다 "고장났다"로만 보인다.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// hitomi의 포맷이 바뀌어 tsuburu 업데이트가 필요하다.
    FormatChanged,
    /// hitomi에 닿지 못했다.
    Network,
    /// 요청이 잘못됐다.
    BadRequest,
    /// 로컬 라이브러리를 읽거나 쓰지 못했다. 검색은 계속 동작한다.
    Storage,
    /// 이 플랫폼에는 없는 기능이다.
    Unsupported,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ApiError {
    pub error: ErrorKind,
    pub message: String,
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self { error: ErrorKind::BadRequest, message: message.into() }
    }

    fn status(&self) -> StatusCode {
        match self.error {
            ErrorKind::FormatChanged => StatusCode::BAD_GATEWAY,
            ErrorKind::Network => StatusCode::SERVICE_UNAVAILABLE,
            ErrorKind::BadRequest => StatusCode::BAD_REQUEST,
            ErrorKind::Storage => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorKind::Unsupported => StatusCode::NOT_IMPLEMENTED,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status(), Json(self)).into_response()
    }
}

impl From<tsuburu_hitomi::SearchError> for ApiError {
    fn from(err: tsuburu_hitomi::SearchError) -> Self {
        let error =
            if err.is_format_error() { ErrorKind::FormatChanged } else { ErrorKind::Network };
        Self { error, message: describe(error, &err.to_string()) }
    }
}

impl From<tsuburu_hitomi::GalleryFetchError> for ApiError {
    fn from(err: tsuburu_hitomi::GalleryFetchError) -> Self {
        let error =
            if err.is_format_error() { ErrorKind::FormatChanged } else { ErrorKind::Network };
        Self { error, message: describe(error, &err.to_string()) }
    }
}

impl From<tsuburu_hitomi::FetchError> for ApiError {
    fn from(err: tsuburu_hitomi::FetchError) -> Self {
        Self { error: ErrorKind::Network, message: describe(ErrorKind::Network, &err.to_string()) }
    }
}

impl From<tsuburu_hitomi::ImageError> for ApiError {
    fn from(err: tsuburu_hitomi::ImageError) -> Self {
        Self {
            error: ErrorKind::FormatChanged,
            message: describe(ErrorKind::FormatChanged, &err.to_string()),
        }
    }
}

fn describe(kind: ErrorKind, detail: &str) -> String {
    match kind {
        ErrorKind::FormatChanged => {
            format!(
                "hitomi's format appears to have changed, so tsuburu needs an update ({detail})"
            )
        }
        ErrorKind::Network => format!("could not reach hitomi ({detail})"),
        ErrorKind::BadRequest => detail.to_string(),
        ErrorKind::Storage => format!("the local library could not be used ({detail})"),
        ErrorKind::Unsupported => detail.to_string(),
    }
}

impl From<tsuburu_downloads::DownloadError> for ApiError {
    fn from(err: tsuburu_downloads::DownloadError) -> Self {
        Self { error: ErrorKind::Storage, message: describe(ErrorKind::Storage, &err.to_string()) }
    }
}

impl From<tsuburu_dialogue::DialogueError> for ApiError {
    fn from(err: tsuburu_dialogue::DialogueError) -> Self {
        Self { error: ErrorKind::Storage, message: describe(ErrorKind::Storage, &err.to_string()) }
    }
}

impl From<tsuburu_store::StoreError> for ApiError {
    fn from(err: tsuburu_store::StoreError) -> Self {
        Self { error: ErrorKind::Storage, message: describe(ErrorKind::Storage, &err.to_string()) }
    }
}
