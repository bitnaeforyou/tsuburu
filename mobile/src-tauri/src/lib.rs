//! tsuburu on a phone.
//!
//! The same program, in a window the operating system gave it rather than one
//! a browser did. The server runs inside the app on a loopback port nobody
//! else can reach, and the webview is pointed at it - so the interface, which
//! already knows how to be held in one hand, is the one that ships. Nothing
//! is talking to a machine somewhere else; the phone is the machine.

use std::sync::Arc;

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

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

    // Loopback only, and a port of the system's choosing. Nothing outside the
    // phone can open either.
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
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
