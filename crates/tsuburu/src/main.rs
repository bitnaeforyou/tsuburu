use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::time::Instant;
use tsuburu_fetch::{FetchConfig, HttpFetcher};
use tsuburu_hitomi::Config;

#[derive(Parser)]
#[command(name = "tsuburu", version, about = "A lightweight hitomi search client")]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// 로그 상세도를 높인다.
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Command {
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

    let fetcher = HttpFetcher::new(FetchConfig::default())
        .context("failed to build the HTTP client")?;
    let cfg = Config::default();

    match cli.command {
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
