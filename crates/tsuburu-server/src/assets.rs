//! 빌드된 프론트엔드를 바이너리에 심어 서빙한다.
//!
//! 사용자가 받는 것이 파일 하나여야 하므로 정적 자산을 따로 배포하지 않는다.

use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../../web/dist"]
struct Assets;

pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    if let Some(file) = Assets::get(path) {
        return respond(path, file);
    }

    // 해시 라우터를 쓰므로 실제로는 거의 오지 않지만, 알 수 없는 경로는
    // 앱 껍데기로 돌려보낸다.
    match Assets::get("index.html") {
        Some(file) => respond("index.html", file),
        None => (StatusCode::NOT_FOUND, "frontend assets are missing; run `npm run build` in web/")
            .into_response(),
    }
}

fn respond(path: &str, file: rust_embed::EmbeddedFile) -> Response {
    ([(header::CONTENT_TYPE, kind_of(path))], file.data.into_owned()).into_response()
}

/// What the build puts in `web/dist`, and nothing else.
///
/// The whole of it is one page, one stylesheet and one script; a table of
/// every extension anyone has registered is a lot of program to answer three
/// questions with.
fn kind_of(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}
