//! localhost에서 도는 HTTP 서버.

pub mod api;
pub mod assets;
pub mod dialogue;
pub mod grinder;
pub mod error;
pub mod library;
pub mod meta;
pub mod proxy;
pub mod state;

use axum::Router;
use axum::routing::{get, post, put};
use std::sync::Arc;
use tower_http::trace::TraceLayer;

pub use state::AppState;

/// A shard of a whole language is a few hundred megabytes at most.
const SHARD_UPLOAD_LIMIT: usize = 1024 * 1024 * 1024;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/search", get(api::search))
        .route("/api/cards", get(api::cards))
        .route("/api/gallery/{id}", get(api::gallery))
        .route("/api/favorites", get(library::list_favorites))
        .route("/api/favorites/{id}", put(library::add_favorite).delete(library::remove_favorite))
        .route("/api/history", get(library::list_history).delete(library::clear_history))
        .route("/api/history/{id}", put(library::record_progress))
        .route("/api/meta/status", get(meta::status))
        .route("/api/meta/search", get(meta::search))
        .route("/api/meta/suggest", get(meta::suggest))
        .route("/api/dialogue/status", get(dialogue::status))
        .route("/api/dialogue/settings", put(dialogue::update_settings))
        .route("/api/dialogue/search", get(dialogue::search))
        .route("/api/dialogue/enqueue", post(dialogue::enqueue))
        .route("/api/dialogue/hunt", post(dialogue::hunt))
        .route("/api/dialogue/import-artifact", post(dialogue::import_artifact))
        .route("/api/dialogue/export", post(dialogue::export))
        .route("/api/dialogue/shards", get(dialogue::list_shards))
        .route("/api/dialogue/shards/{name}", get(dialogue::download_shard))
        .route(
            "/api/dialogue/import",
            post(dialogue::import).layer(axum::extract::DefaultBodyLimit::max(SHARD_UPLOAD_LIMIT)),
        )
        .route("/img/{file}", get(proxy::image))
        .route("/tn/{file}", get(proxy::thumbnail))
        .fallback(assets::serve)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
