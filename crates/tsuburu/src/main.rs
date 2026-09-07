use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::time::Instant;
use tsuburu_fetch::{FetchConfig, HttpFetcher};
use tsuburu_hitomi::Config;

#[derive(Parser)]
#[command(name = "tsuburu", version, about = "A lightweight hitomi search client")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// 로그 상세도를 높인다.
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Command {
    /// 로컬 서버를 띄우고 브라우저를 연다. 인자 없이 실행하면 이것이 기본이다.
    Serve {
        /// 0이면 비어 있는 포트를 자동으로 잡는다.
        #[arg(short, long, default_value_t = 8420)]
        port: u16,

        /// 브라우저를 자동으로 열지 않는다.
        #[arg(long)]
        no_open: bool,

        /// 배경에서 미리 받아둘 인덱스 깊이. 0이면 예열하지 않는다.
        #[arg(long, default_value_t = WARM_LEVELS)]
        warm_levels: usize,
    },
    /// 갤러리를 검색해 ID를 출력한다. `-태그`로 제외할 수 있다.
    Search {
        /// 검색어. 여러 개를 주면 AND로 묶인다. 제외 항목이 있으면
        /// 셸이 플래그로 오해하지 않도록 따옴표로 묶는다:
        /// `tsuburu search "glasses -school"`
        query: Vec<String>,

        #[arg(short, long, default_value_t = 25)]
        limit: usize,
    },
    /// 갤러리 한 편의 메타데이터와 이미지 URL을 출력한다.
    Gallery {
        id: i32,

        /// 출력할 이미지 URL 개수.
        #[arg(short, long, default_value_t = 3)]
        images: usize,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    // 인자 없이 실행하는 것이 일반 사용자의 경로다. 더블클릭하면 서버가 뜬다.
    let command = cli
        .command
        .unwrap_or(Command::Serve { port: 8420, no_open: false, warm_levels: WARM_LEVELS });

    let fetcher = HttpFetcher::new(FetchConfig::default())
        .context("failed to build the HTTP client")?;
    let cfg = Config::default();

    match command {
        Command::Serve { port, no_open, warm_levels } => {
            serve(fetcher, cfg, port, no_open, warm_levels).await?;
        }

        Command::Search { query, limit } => {
            let started = Instant::now();
            let version = tsuburu_hitomi::galleries_index_version(&fetcher, &cfg)
                .await
                .context("could not read the index version")?;
            tracing::debug!(version, "galleries index");

            let parsed = tsuburu_hitomi::parse_query(&query.join(" "));
            if parsed.include.is_empty() {
                anyhow::bail!("give at least one search term that is not an exclusion");
            }

            let ids = tsuburu_hitomi::search(&fetcher, &cfg, &version, &parsed, Some(limit))
                .await
                .map_err(describe)?;

            for id in &ids {
                println!("{id}");
            }

            let stats = fetcher.stats();
            eprintln!(
                "{} ids in {:?} — {} requests, {} cache hits, {} bytes",
                ids.len(),
                started.elapsed(),
                stats.requests,
                stats.cache_hits,
                stats.bytes
            );
        }

        Command::Gallery { id, images } => {
            let gallery = tsuburu_hitomi::fetch_gallery(&fetcher, &cfg, id)
                .await
                .map_err(describe_gallery)?;
            let gg = tsuburu_hitomi::fetch_gg(&fetcher, &cfg)
                .await
                .map_err(describe_gallery)?;

            println!("title: {}", gallery.title.as_deref().unwrap_or("(none)"));
            println!("type:  {}", gallery.kind.as_deref().unwrap_or("(none)"));
            println!("lang:  {}", gallery.language.as_deref().unwrap_or("(none)"));
            println!("pages: {}", gallery.files.len());
            if !gallery.tags.is_empty() {
                println!("tags:  {}", gallery.tags.join(", "));
            }
            for file in gallery.files.iter().take(images) {
                println!("{}", tsuburu_hitomi::image_url(&cfg, &gg, file)?);
            }
        }
    }

    Ok(())
}

/// 예열할 인덱스 깊이의 기본값. 한 레벨이 노드 17개이므로 1이면 18개,
/// 2면 300개 남짓을 받는다.
const WARM_LEVELS: usize = 2;

async fn serve(
    fetcher: HttpFetcher,
    cfg: Config,
    port: u16,
    no_open: bool,
    warm_levels: usize,
) -> Result<()> {
    use std::sync::Arc;

    // 라이브러리를 열지 못해도 검색은 계속 동작해야 한다(스펙 7절).
    let store = match tsuburu_store::Store::open_default() {
        Ok(store) => {
            tracing::info!(path = %store.path().display(), "opened the library");
            Some(store)
        }
        Err(err) => {
            eprintln!("favorites and history are disabled: {err}");
            None
        }
    };

    let fetcher = Arc::new(fetcher);

    // Dialogue indexing needs the platform's OCR. Where there is none the
    // feature is reported as unsupported rather than silently missing.
    let grinder = match tsuburu_ocr::platform_ocr(tsuburu_ocr::OcrOptions::default()) {
        None => {
            tracing::info!("no text recognition on this platform; dialogue search disabled");
            None
        }
        Some(ocr) => match tsuburu_store::data_dir()
            .map_err(|e| e.to_string())
            .and_then(|dir| tsuburu_dialogue::DialogueStore::open(dir.join("dialogue.redb")).map_err(|e| e.to_string()))
        {
            Ok(dialogue) => {
                tracing::info!(path = %dialogue.path().display(), "opened the dialogue index");
                // Its own connection pool: a page burst must not queue behind
                // the index warm-up, and must never slow down browsing.
                let grinder_fetcher = HttpFetcher::new(FetchConfig {
                    max_concurrent: 8,
                    cache_entries: 256,
                    ..FetchConfig::default()
                })
                .context("failed to build the indexing HTTP client")?;
                Some(Arc::new(tsuburu_server::grinder::Grinder::new(
                    Arc::new(grinder_fetcher),
                    cfg.clone(),
                    Arc::new(dialogue),
                    Arc::from(ocr),
                )))
            }
            Err(err) => {
                eprintln!("dialogue search is disabled: {err}");
                None
            }
        },
    };
    if let Some(grinder) = &grinder {
        tokio::spawn(Arc::clone(grinder).run());
    }

    let state = Arc::new(tsuburu_server::AppState::full(fetcher, cfg, store, grinder));
    let app = tsuburu_server::router(Arc::clone(&state));

    // 상위 노드 예열은 배경에서 돌린다. 서버 기동을 막지 않는다.
    if warm_levels > 0 {
        let state = Arc::clone(&state);
        tokio::spawn(async move { state.warm(warm_levels).await });
    }

    let listener = bind(port).await?;
    let addr = listener.local_addr().context("could not read the local address")?;
    let url = format!("http://127.0.0.1:{}/", addr.port());

    println!("tsuburu is running at {url}");
    println!("press ctrl+c to stop");

    if !no_open && let Err(err) = open::that_detached(&url) {
        // 브라우저를 못 여는 것은 치명적이지 않다. 주소를 이미 출력했다.
        eprintln!("could not open a browser automatically ({err}); open {url} yourself");
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .context("the server stopped unexpectedly")?;

    if let Some(grinder) = &state.grinder {
        grinder.shutdown();
    }
    Ok(())
}

/// 원하는 포트가 이미 쓰이고 있으면 아무 빈 포트나 잡는다. 일반 사용자에게
/// "포트가 사용 중입니다"라고 말하고 끝내는 것은 도움이 되지 않는다.
async fn bind(port: u16) -> Result<tokio::net::TcpListener> {
    if port != 0 {
        match tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
            Ok(listener) => return Ok(listener),
            Err(err) => eprintln!("port {port} is not available ({err}); picking another"),
        }
    }
    tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .context("could not bind a local port")
}

/// 포맷 변경과 네트워크 오류를 구분해 안내한다(스펙 7절).
fn describe(err: tsuburu_hitomi::SearchError) -> anyhow::Error {
    if err.is_format_error() {
        anyhow::anyhow!(
            "hitomi's index format appears to have changed, so tsuburu needs an update ({err})"
        )
    } else {
        anyhow::anyhow!("could not reach hitomi ({err})")
    }
}

fn describe_gallery(err: tsuburu_hitomi::GalleryFetchError) -> anyhow::Error {
    if err.is_format_error() {
        anyhow::anyhow!(
            "hitomi's gallery format appears to have changed, so tsuburu needs an update ({err})"
        )
    } else {
        anyhow::anyhow!("could not reach hitomi ({err})")
    }
}

fn init_tracing(verbose: bool) {
    let default = if verbose { "tsuburu=debug,tsuburu_fetch=debug" } else { "warn" };
    let filter = tracing_subscriber::EnvFilter::try_from_env("TSUBURU_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}
