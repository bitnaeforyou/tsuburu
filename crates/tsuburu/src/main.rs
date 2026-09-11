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
    /// `llm-search-index` 디렉터리에서 대사 텍스트를 가져온다.
    /// 서버가 떠 있으면 저장소가 잠겨 있으므로 먼저 끄거나 UI에서 실행한다.
    ImportArtifact { dir: std::path::PathBuf },
    /// `data.db`(SQLite)에서 갤러리 메타데이터 스냅샷을 가져온다.
    /// `sqlite3` 명령으로 읽으므로 그 명령이 있어야 한다. 서버를 먼저 끈다.
    ImportMeta {
        db: std::path::PathBuf,

        /// hitomi에서 이미 내려간 작품까지 포함한다.
        #[arg(long)]
        all: bool,
    },
    /// `graph.csv`에서 작품별 키워드를 가져온다. 서버를 먼저 끈다.
    ImportKeywords { csv: std::path::PathBuf },
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
    let command = cli.command.unwrap_or(Command::Serve {
        port: 8420,
        no_open: false,
        warm_levels: WARM_LEVELS,
    });

    let fetcher =
        HttpFetcher::new(FetchConfig::default()).context("failed to build the HTTP client")?;
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

        Command::ImportArtifact { dir } => {
            let path = tsuburu_store::data_dir()?.join("dialogue.redb");
            let store = tsuburu_dialogue::DialogueStore::open(&path)
                .map_err(|e| anyhow::anyhow!("{e} (is the server running? stop it first)"))?;
            let reader = tsuburu_dialogue::artifact::ChunkReader::open(&dir)?;
            store.set_artifact_dir(&dir)?;
            let total = reader.len();
            eprintln!("importing {total} chunks from {}", dir.display());
            let started = Instant::now();
            let works =
                tsuburu_dialogue::artifact::WorkGrouper::new(reader).filter_map(|w| match w {
                    Ok(w) => Some(w),
                    Err(err) => {
                        eprintln!("skipping: {err}");
                        None
                    }
                });
            let summary = store.import_works(works, "artifact", 500, |seen| {
                if seen % 5000 == 0 {
                    eprintln!("  {seen} works, {:?}", started.elapsed());
                }
            })?;
            eprintln!(
                "done in {:?}: {} works, {} added, {} already present",
                started.elapsed(),
                summary.galleries,
                summary.added,
                summary.skipped
            );
        }

        Command::ImportMeta { db, all } => {
            let path = tsuburu_store::data_dir()?.join("meta.redb");
            let store = tsuburu_meta::MetaStore::open(&path)
                .map_err(|e| anyhow::anyhow!("{e} (is the server running? stop it first)"))?;
            let started = Instant::now();
            let rows = artifact_meta::read_rows(&db, all)?;
            let summary = store.import_works(rows, 2_000, |seen| {
                if seen % 100_000 == 0 {
                    eprintln!("  {seen} works, {:?}", started.elapsed());
                }
            })?;
            eprintln!("done in {:?}: {} works", started.elapsed(), summary.works);
        }

        Command::ImportKeywords { csv } => {
            let path = tsuburu_store::data_dir()?.join("keywords.redb");
            let store = tsuburu_keywords::KeywordStore::open(&path)
                .map_err(|e| anyhow::anyhow!("{e} (is the server running? stop it first)"))?;
            let started = Instant::now();
            eprintln!("reading {}", csv.display());
            let done = tsuburu_keywords::graph::import(&store, &csv, &mut |p| {
                if p.works % 20_000 == 0 {
                    eprintln!("  {} works, {:?}", p.works, started.elapsed());
                }
            })?;
            eprintln!(
                "done in {:?}: {} works, {} words ({} too common to keep)",
                started.elapsed(),
                done.works,
                done.words,
                done.too_common
            );
        }

        Command::Gallery { id, images } => {
            let gallery = tsuburu_hitomi::fetch_gallery(&fetcher, &cfg, id)
                .await
                .map_err(describe_gallery)?;
            let gg = tsuburu_hitomi::fetch_gg(&fetcher, &cfg).await.map_err(describe_gallery)?;

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
        // One copy at a time: the databases take a lock. Starting a second
        // one would come up with favorites, history, dialogue and downloads
        // all switched off, which looks like a broken program rather than a
        // program that is already running.
        Err(err) if already_running(&err.to_string()) => {
            eprintln!("tsuburu is already running.");
            eprintln!("Open http://127.0.0.1:{port}/ , or stop the other one first.");
            std::process::exit(1);
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
        Some(ocr) => match tsuburu_store::data_dir().map_err(|e| e.to_string()).and_then(|dir| {
            tsuburu_dialogue::DialogueStore::open(dir.join("dialogue.redb"))
                .map_err(|e| e.to_string())
        }) {
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
                Some(Arc::new(tsuburu_server::grinder::Grinder::with_factory(
                    Arc::new(grinder_fetcher),
                    cfg.clone(),
                    Arc::new(dialogue),
                    Arc::from(ocr),
                    tsuburu_ocr::platform_ocr,
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

    let mut app_state = tsuburu_server::AppState::full(fetcher, cfg, store, grinder);
    if let Ok(dir) = tsuburu_store::data_dir() {
        app_state = app_state.with_shards_dir(dir.join("shards"));
        match tsuburu_downloads::DownloadStore::open(&dir) {
            Ok(downloads) => {
                tracing::info!(path = %downloads.images_dir().display(), "opened downloads");
                app_state = app_state.with_downloads(Arc::new(downloads));
            }
            Err(err) => eprintln!("downloads are disabled: {err}"),
        }
        let keywords_path = dir.join("keywords.redb");
        if keywords_path.is_file() {
            match tsuburu_keywords::KeywordStore::open(&keywords_path) {
                Ok(keywords) => app_state = app_state.with_keywords(Arc::new(keywords)),
                Err(err) => eprintln!("keywords are unavailable: {err}"),
            }
        }
        let meta_path = dir.join("meta.redb");
        if meta_path.is_file() {
            match tsuburu_meta::MetaStore::open(&meta_path) {
                Ok(meta) => {
                    tracing::info!(path = %meta_path.display(), "opened the metadata snapshot");
                    app_state = app_state.with_meta(Arc::new(meta));
                }
                Err(err) => eprintln!("metadata snapshot is disabled: {err}"),
            }
        }
    }
    let state = Arc::new(app_state);
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
        .with_graceful_shutdown(stop_signal())
        .await
        .context("the server stopped unexpectedly")?;

    if let Some(grinder) = &state.grinder {
        grinder.shutdown();
    }
    Ok(())
}

/// Whether a database refused to open because another copy holds it.
///
/// redb says so in words rather than a distinct error, so this reads them.
/// Being wrong in the cautious direction only costs a clearer message.
fn already_running(message: &str) -> bool {
    let message = message.to_lowercase();
    message.contains("already open") || message.contains("acquire lock")
}

/// Ctrl-C, or the TERM a process manager sends.
///
/// Both have to be caught: redb repairs a file that was not closed, and on a
/// corpus this size that costs half a minute and two gigabytes of memory the
/// next time the program starts. Waiting on Ctrl-C alone meant every `kill`
/// left the databases dirty.
async fn stop_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = term.recv() => {}
                }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
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
    tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.context("could not bind a local port")
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
    tracing_subscriber::fmt().with_env_filter(filter).with_writer(std::io::stderr).init();
}

/// Reading a `data.db` through the `sqlite3` command.
///
/// The file is a SQLite database with FTS5 virtual tables. Linking a SQL
/// engine into the binary would either bring in C or a large Rust crate for
/// a one-time import, so the local `sqlite3` tool streams the rows out
/// instead, with unit/record separators that cannot appear in the data.
mod artifact_meta {
    use anyhow::{Context, Result};
    use std::io::{BufRead, BufReader};
    use std::path::Path;
    use std::process::{Command, Stdio};
    use tsuburu_meta::Work;

    const FIELD: u8 = 0x1f;
    const RECORD: u8 = 0x1e;

    pub fn read_rows(db: &Path, all: bool) -> Result<impl Iterator<Item = Work>> {
        let filter = if all { "" } else { " WHERE ExistOnHitomi = 1" };
        let sql = format!(
            "SELECT Id, Title, Type, Language, Artists, Groups, Series, Characters, Tags, \
             Published, Files, ExistOnHitomi, Thumbnail FROM HitomiColumnModel{filter} ORDER BY Id"
        );
        let mut child = Command::new("sqlite3")
            .arg("-readonly")
            .arg("-list")
            .arg("-noheader")
            .arg("-separator")
            .arg(String::from_utf8_lossy(&[FIELD]).to_string())
            .arg("-newline")
            .arg(String::from_utf8_lossy(&[RECORD]).to_string())
            .arg(db)
            .arg(&sql)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context(
                "could not run `sqlite3`. macOS ships it; on Linux install the sqlite3 \
                 package, and on Windows put sqlite3.exe from sqlite.org on PATH",
            )?;
        let stdout = child.stdout.take().context("no stdout from sqlite3")?;
        let reader = BufReader::with_capacity(4 << 20, stdout);
        Ok(reader
            .split(RECORD)
            .filter_map(|record| record.ok())
            .filter(|record| !record.is_empty())
            .filter_map(|record| parse_row(&record)))
    }

    fn parse_row(record: &[u8]) -> Option<Work> {
        let fields: Vec<String> = record
            .split(|&b| b == FIELD)
            .map(|f| String::from_utf8_lossy(f).into_owned())
            .collect();
        if fields.len() < 13 {
            return None;
        }
        let id: i32 = fields[0].trim().parse().ok()?;
        Some(Work {
            id,
            title: fields[1].trim().to_string(),
            kind: normalize_kind(&fields[2]),
            language: fields[3].trim().to_lowercase(),
            artists: split_list(&fields[4]),
            groups: split_list(&fields[5]),
            series: split_list(&fields[6]),
            characters: split_list(&fields[7]),
            tags: split_list(&fields[8]),
            published: parse_published(&fields[9]),
            pages: fields[10].trim().parse().unwrap_or(0),
            exists: fields[11].trim() == "1",
            thumbnail_hash: thumbnail_hash(&fields[12]),
        })
    }

    /// `|a|b|` -> ["a", "b"]
    fn split_list(s: &str) -> Vec<String> {
        s.split('|').map(str::trim).filter(|p| !p.is_empty()).map(str::to_string).collect()
    }

    /// The source spells types inconsistently ("artist CG", "artist cg");
    /// hitomi's own list names are the canonical form.
    fn normalize_kind(s: &str) -> String {
        s.trim().to_lowercase().replace(' ', "")
    }

    /// Either "YYYY-MM-DD HH:MM:SS" or .NET ticks (100 ns since year 1).
    fn parse_published(s: &str) -> Option<i64> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        if let Ok(ticks) = s.parse::<i64>() {
            const UNIX_EPOCH_TICKS: i64 = 621_355_968_000_000_000;
            return Some((ticks - UNIX_EPOCH_TICKS) / 10_000_000);
        }
        let (date, time) = s.split_once(' ')?;
        let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
        let (y, m, day) = (d.next()??, d.next()??, d.next()??);
        let mut t = time.split(':').map(|p| p.parse::<i64>().ok());
        let (h, mi, sec) = (t.next()??, t.next()??, t.next().flatten().unwrap_or(0));
        Some(days_from_civil(y, m, day) * 86_400 + h * 3_600 + mi * 60 + sec)
    }

    /// Howard Hinnant's algorithm; avoids a date crate for one field.
    fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
        let y = if m <= 2 { y - 1 } else { y };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let mp = (m + 9) % 12;
        let doy = (153 * mp + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// `//tn.gold-usergeneratedcontent.net/webpbigtn/a/30/<64 hex>.webp` -> hash
    fn thumbnail_hash(url: &str) -> Option<String> {
        let name = url.rsplit('/').next()?;
        let stem = name.split('.').next()?;
        (stem.len() == 64 && stem.chars().all(|c| c.is_ascii_hexdigit())).then(|| stem.to_string())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parses_both_date_forms() {
            assert_eq!(parse_published("2026-07-04 15:47:00"), Some(1_783_180_020));
            // 633309743400000000 ticks = 2007-11-18 10:39:00 UTC
            assert_eq!(parse_published("633309743400000000"), Some(1_195_377_540));
            assert_eq!(parse_published(""), None);
        }

        #[test]
        fn parses_a_row() {
            let fields = [
                "4031231",
                "Drip Coffee | 드립 커피",
                "manga",
                "korean",
                "|borusiti|",
                "",
                "",
                "",
                "|female:big breasts|digital|",
                "2026-07-04 15:47:00",
                "40",
                "1",
                "//tn.gold-usergeneratedcontent.net/webpbigtn/a/30/753fff08a7c5c60af802a94e56128c4c3cd40791e719ae1c4fa3e89f5526e30a.webp",
            ];
            let record = fields.join(std::str::from_utf8(&[FIELD]).unwrap());
            let w = parse_row(record.as_bytes()).unwrap();
            assert_eq!(w.id, 4031231);
            assert_eq!(w.artists, vec!["borusiti"]);
            assert_eq!(w.tags, vec!["female:big breasts", "digital"]);
            assert_eq!(w.pages, 40);
            assert!(w.exists);
            assert_eq!(w.thumbnail_hash.as_deref().map(|h| h.len()), Some(64));
        }

        #[test]
        fn kinds_are_normalised() {
            assert_eq!(normalize_kind("artist CG"), "artistcg");
            assert_eq!(normalize_kind("image set"), "imageset");
        }
    }
}
