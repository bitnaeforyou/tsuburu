//! localhost에서 도는 HTTP 서버.

pub mod api;
pub mod assets;
pub mod error;
pub mod proxy;
pub mod state;

use axum::Router;
use axum::routing::get;
use std::sync::Arc;
use tower_http::trace::TraceLayer;

pub use state::AppState;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/search", get(api::search))
        .route("/api/cards", get(api::cards))
        .route("/api/gallery/{id}", get(api::gallery))
        .route("/img/{file}", get(proxy::image))
        .route("/tn/{file}", get(proxy::thumbnail))
        .fallback(assets::serve)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
