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
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    ([(header::CONTENT_TYPE, mime.as_ref())], file.data.into_owned()).into_response()
}
