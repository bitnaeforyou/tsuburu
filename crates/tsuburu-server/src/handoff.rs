//! Getting a file the program wrote off the machine it wrote it on.
//!
//! A page in a browser has one way to save a file - a link with `download` on
//! it - and on a computer that is the right one. In the webview a phone shows
//! it does nothing at all: there is no download listener behind it, so the
//! click is dropped without an error, and an export that reported success had
//! written nothing anywhere the reader could reach. So the interface asks what
//! this machine does instead, and the answer is acted on here.

use std::path::PathBuf;
use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};
use tsuburu_host::Handoff;

use crate::error::{ApiError, ErrorKind};
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct HandoffState {
    /// Which of showing a file and sending it this machine does.
    pub handoff: Handoff,
}

/// Said before anything is written, so the interface can offer the one thing
/// that will work rather than the one that happens to be a web standard.
pub async fn state() -> Json<HandoffState> {
    Json(HandoffState { handoff: tsuburu_host::handoff() })
}

#[derive(Debug, Deserialize)]
pub struct HandoffParams {
    /// What to call the file. A plain name: it is joined onto a directory of
    /// our choosing, never used as a path.
    pub name: String,
}

/// Writes a body the page is holding to a file and hands it over.
///
/// For what the page made rather than the server: a library backup is packed
/// in the interface, and reimplementing that packing here to save a round
/// trip would be two formats that have to agree forever.
pub async fn take(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HandoffParams>,
    body: Bytes,
) -> Result<Json<HandoffState>, ApiError> {
    if body.is_empty() {
        return Err(ApiError::bad_request("nothing was sent to hand over"));
    }
    let name = plain_name(&params.name)?;
    let dir = staging(&state)?;
    std::fs::create_dir_all(&dir).map_err(storage)?;
    let path = dir.join(name);
    std::fs::write(&path, &body).map_err(storage)?;
    hand_over(&[path])?;
    Ok(Json(HandoffState { handoff: tsuburu_host::handoff() }))
}

/// Hands paths to the platform, said in terms a reader can act on.
pub fn hand_over(paths: &[PathBuf]) -> Result<(), ApiError> {
    tsuburu_host::hand_over(paths).map_err(|message| ApiError {
        error: ErrorKind::Unsupported,
        message,
        code: Some("no_handoff"),
    })
}

/// A file name and nothing else.
///
/// The name arrives from the interface, and the interface is a web page: a
/// name with a separator or a `..` in it would be a request to write wherever
/// it led. Checked rather than sanitised, because a name that needed cleaning
/// up is a name nobody meant to send.
fn plain_name(name: &str) -> Result<&str, ApiError> {
    let sound = !name.is_empty()
        && name.len() <= 128
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', '\0'])
        && name.chars().all(|c| !c.is_control());
    if sound { Ok(name) } else { Err(ApiError::bad_request("not a file name")) }
}

/// Where a file waits to be handed over.
///
/// Beside the exported shards, which is the directory chosen for the same
/// reason: on a phone it is the one place the file provider may serve out of.
fn staging(state: &AppState) -> Result<PathBuf, ApiError> {
    state.shards_dir.clone().ok_or_else(|| ApiError {
        error: ErrorKind::Storage,
        message: "no directory is configured to write files to".into(),
        code: None,
    })
}

fn storage(error: std::io::Error) -> ApiError {
    ApiError { error: ErrorKind::Storage, message: error.to_string(), code: None }
}
