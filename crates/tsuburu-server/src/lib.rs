//! localhost에서 도는 HTTP 서버.

pub mod api;
pub mod artists;
pub mod assets;
pub mod boot;
pub mod corpus;
pub mod dialogue;
pub mod downloads;
pub mod error;
pub mod grinder;
pub mod keywords;
pub mod library;
pub mod listed;
pub mod meta;
pub mod proxy;
pub mod similar;
pub mod state;
pub mod update;

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
        .route("/api/folders", get(library::list_folders))
        .route("/api/folders/{id}", put(library::set_folder))
        .route("/api/history", get(library::list_history).delete(library::clear_history))
        .route("/api/kept", get(library::kept_cards).delete(library::forget_kept_cards))
        .route("/api/hidden", get(library::hidden_tags).put(library::set_hidden_tags))
        .route("/api/history/{id}", put(library::record_progress))
        .route("/api/artists/following", get(artists::following))
        .route("/api/artists/{name}", get(artists::works))
        .route("/api/series/{name}", get(artists::series))
        .route("/api/artists/{name}/follow", put(artists::follow).delete(artists::unfollow))
        .route("/api/keywords/search", get(keywords::search))
        .route("/api/keywords/{id}", get(keywords::of))
        .route("/api/keywords/{id}/near", get(keywords::near))
        .route("/api/meta/status", get(meta::status))
        .route("/api/meta/search", get(meta::search))
        .route("/api/meta/suggest", get(meta::suggest))
        .route("/api/dialogue/status", get(dialogue::status))
        .route("/api/dialogue/corpus", get(dialogue::corpus_state).post(dialogue::fetch_corpus))
        .route("/api/dialogue/settings", put(dialogue::update_settings))
        .route("/api/dialogue/search", get(dialogue::search))
        .route("/api/dialogue/phrase", get(dialogue::phrase))
        .route("/api/dialogue/pack", get(dialogue::get_embedder).put(dialogue::set_embedder))
        .route("/api/dialogue/pack/check", get(dialogue::check_pack))
        .route("/api/update", get(update::state))
        .route("/api/update/check", post(update::check))
        .route("/api/update/apply", post(update::apply))
        .route("/api/model", get(dialogue::get_model))
        .route("/api/model/enable", post(dialogue::enable_model))
        .route("/api/model/disable", post(dialogue::disable_model))
        .route("/api/dialogue/stored", get(dialogue::stored).delete(dialogue::forget_read))
        .route("/api/dialogue/stored/{id}", axum::routing::delete(dialogue::forget))
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
        .route("/api/downloads", get(downloads::list))
        .route(
            "/api/downloads/{id}",
            post(downloads::start).get(downloads::status).delete(downloads::remove),
        )
        .route("/img/{file}", get(proxy::image))
        .route("/tn/{file}", get(proxy::thumbnail))
        .fallback(assets::serve)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
