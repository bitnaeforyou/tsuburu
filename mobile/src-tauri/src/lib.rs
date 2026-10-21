//! tsuburu on a phone.
//!
//! The same program, in a window the operating system gave it rather than one
//! a browser did. The server runs inside the app on a loopback port nobody
//! else can reach, and the webview is pointed at it - so the interface, which
//! already knows how to be held in one hand, is the one that ships. Nothing
//! is talking to a machine somewhere else; the phone is the machine.

use std::sync::{Arc, OnceLock};

use axum::extract::Request;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use axum::Router;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use tower::ServiceExt;

/// Android hands the Java machine to a library as it loads it, and never
/// offers it again. Anything that reaches another application needs it -
/// replacing this app through the system installer, handing an exported file
/// to the share sheet - because reaching one is a Java call.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn JNI_OnLoad(vm: *mut std::ffi::c_void, _reserved: *mut std::ffi::c_void) -> i32 {
    tsuburu_host::note_machine(vm);
    // JNI_VERSION_1_6, the oldest every Android supports.
    0x0001_0006
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tsuburu_server=info,tsuburu_mobile_lib=info".into()),
        )
        .init();

    tauri::Builder::default()
        .setup(|app| {
            // Where this phone lets the app keep things. `directories` has
            // nothing to go on here - there is no home directory - so the
            // place the operating system gave us is passed along.
            if let Ok(dir) = app.path().app_data_dir() {
                std::fs::create_dir_all(&dir).ok();
                // SAFETY: setup runs before anything else is spawned, so
                // nothing is reading the environment while this writes it.
                unsafe { std::env::set_var("TSUBURU_DATA_DIR", &dir) };
                tracing::info!(path = %dir.display(), "keeping things here");
            }
            // The update puts the package it fetched here: it is the one
            // directory the FileProvider is allowed to share out of, which is
            // how the system installer gets to read it.
            if let Ok(dir) = app.path().app_cache_dir() {
                std::fs::create_dir_all(&dir).ok();
                // SAFETY: as above - nothing else is running yet.
                unsafe { std::env::set_var("TSUBURU_CACHE_DIR", &dir) };
            }
            let listener = tauri::async_runtime::block_on(bind())?;
            let address = listener.local_addr()?;
            tauri::async_runtime::spawn(serve(listener));
            let url = format!("http://{address}/");
            tracing::info!(%url, "serving");
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url.parse()?))
                .title("tsuburu")
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("tsuburu could not start");
}

/// What the address answers while the stores are still being opened.
const OPENING: &str = include_str!("opening.html");

/// Serves the window straight away, and the interface once there is one.
///
/// Opening the stores is not always quick: a database the phone closed by
/// ending the process is checked through before it opens, and after an
/// update that is all of a two-gigabyte dialogue index. Waiting for it before
/// the window existed showed nothing for minutes, on the thread Android
/// expects to answer it.
async fn serve(listener: tokio::net::TcpListener) {
    let opened: Arc<OnceLock<Result<Router, String>>> = Arc::new(OnceLock::new());
    let front = Router::new().fallback({
        let opened = Arc::clone(&opened);
        move |request: Request| {
            let opened = Arc::clone(&opened);
            async move {
                match opened.get() {
                    Some(Ok(router)) => router.clone().oneshot(request).await.into_response(),
                    Some(Err(error)) => (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("tsuburu could not start: {error}"),
                    )
                        .into_response(),
                    None => (StatusCode::SERVICE_UNAVAILABLE, Html(OPENING)).into_response(),
                }
            }
        }
    });
    tauri::async_runtime::spawn(async move {
        if let Err(err) = axum::serve(listener, front).await {
            tracing::error!(%err, "the server stopped");
        }
    });

    // On a thread of its own, because opening a store is reading a file, not
    // waiting on one. Run here it held this worker for as long as it took,
    // and the server spawned just above was queued on the same worker where
    // no other could take it: the address answered nothing at all until the
    // stores were open, which is what the page was there to cover.
    let assembled = tauri::async_runtime::spawn_blocking(|| {
        tauri::async_runtime::block_on(tsuburu_server::boot::assemble(
            tsuburu_hitomi::Config::default(),
        ))
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|assembled| assembled.map_err(|e| e.to_string()));
    match assembled {
        Ok(state) => {
            // The phone binds loopback and nothing else can reach it.
            let router = tsuburu_server::router(
                Arc::clone(&state),
                tsuburu_server::guard::Reach::ThisMachine,
            );
            opened.set(Ok(router)).ok();
            tracing::info!("opened");
            // The top of the index is worth having before the first screen is drawn.
            state.warm(2).await;
        }
        Err(err) => {
            tracing::error!(%err, "tsuburu could not start");
            opened.set(Err(err)).ok();
        }
    }
}

/// Loopback only, on the same port as last time if it can be had.
///
/// The interface keeps what the reader chose - the age they confirmed, how
/// they read, what they looked for lately - in the webview's own storage,
/// which belongs to an origin, and an origin is a port. A port of the
/// system's choosing is a different one every launch, so every launch opened
/// a phone that had never been used before. Nothing outside the phone can
/// reach any of these.
async fn bind() -> std::io::Result<tokio::net::TcpListener> {
    for port in 8420..8430 {
        if let Ok(listener) = tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
            return Ok(listener);
        }
    }
    tokio::net::TcpListener::bind(("127.0.0.1", 0)).await
}
