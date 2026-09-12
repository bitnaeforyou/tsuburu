//! Saying that a newer tsuburu exists, and becoming it when asked.

use std::sync::Arc;

use axum::{Json, extract::State};
use serde::Serialize;

use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct UpdateState {
    #[serde(flatten)]
    pub progress: tsuburu_update::Progress,
    /// What is running now.
    pub here: &'static str,
    /// Whether this copy came from a release at all. A build from a checkout
    /// has nowhere to ask and nothing to offer.
    pub published: bool,
}

fn state_of(state: &AppState) -> UpdateState {
    UpdateState {
        progress: state.updater.progress(),
        here: tsuburu_update::HERE,
        published: state.updater.publishable(),
    }
}

pub async fn state(State(state): State<Arc<AppState>>) -> Json<UpdateState> {
    Json(state_of(&state))
}

pub async fn check(State(state): State<Arc<AppState>>) -> Json<UpdateState> {
    state.updater.check();
    Json(state_of(&state))
}

pub async fn apply(State(state): State<Arc<AppState>>) -> Json<UpdateState> {
    state.updater.apply();
    Json(state_of(&state))
}
