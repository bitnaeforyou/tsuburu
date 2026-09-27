//! tsuburu on a phone.
//!
//! The same program, in a window the operating system gave it rather than one
//! a browser did. The server runs inside the app on a loopback port nobody
//! else can reach, and the webview is pointed at it - so the interface, which
//! already knows how to be held in one hand, is the one that ships. Nothing
//! is talking to a machine somewhere else; the phone is the machine.

use std::sync::Arc;

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

/// Android hands the Java machine to a library as it loads it, and never
/// offers it again. The update needs it: replacing this app is done by asking
/// the system installer, and asking is a Java call.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn JNI_OnLoad(vm: *mut std::ffi::c_void, _reserved: *mut std::ffi::c_void) -> i32 {
    tsuburu_update::note_machine(vm);
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
            // Port 0: the operating system picks one that is free. A phone has
            // no terminal to tell a clash to, and the address is handed
            // straight to the webview anyway, so nothing needs to guess it.
            let address = tauri::async_runtime::block_on(start())?;
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

/// Opens the stores, binds a loopback port and serves on it, returning the
/// address it settled on.
async fn start() -> Result<std::net::SocketAddr, Box<dyn std::error::Error>> {
    let cfg = tsuburu_hitomi::Config::default();
    let state = tsuburu_server::boot::assemble(cfg).await.map_err(|e| e.to_string())?;
    let router = tsuburu_server::router(Arc::clone(&state));

    let listener = bind().await?;
    let address = listener.local_addr()?;

    tauri::async_runtime::spawn(async move {
        if let Err(err) = axum::serve(listener, router).await {
            tracing::error!(%err, "the server stopped");
        }
    });

    // The top of the index is worth having before the first screen is drawn.
    tauri::async_runtime::spawn(async move { state.warm(2).await });
    Ok(address)
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
