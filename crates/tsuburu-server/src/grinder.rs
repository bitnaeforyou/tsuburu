//! The background worker that turns pages into searchable text.
//!
//! One gallery at a time: fetch its metadata, stream the pages down under a
//! bandwidth cap, recognise each one on a blocking thread, and commit the
//! text. Images never touch the disk. When the queue runs dry it refills
//! itself from the popularity list, so the galleries most likely to have
//! been read are indexed first (spec, section 3).

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore, mpsc};
use tsuburu_dialogue::artifact::{ChunkReader, WorkGrouper};
use tsuburu_dialogue::{DialogueStore, PageText, Priority};
use tsuburu_fetch::HttpFetcher;
use tsuburu_hitomi::Fetcher;
use tsuburu_hitomi::{Config, Sort, nozomi};
use tsuburu_ocr::{Ocr, reading_order};

/// Vision saturates the Neural Engine; more than this just queues.
const OCR_WORKERS: usize = 3;
/// Pages of a work being read that are recognised at once. Reading has to
/// stay smooth, so this is smaller than the sweep's.
const READ_WORKERS: usize = 2;
/// Pages buffered between the downloader and recognition.
const PIPELINE_DEPTH: usize = 4;
/// Page fetches in flight. The CDN throttles each connection to roughly
/// 150 KB/s, so parallel connections are the only way to keep recognition
/// fed; measured aggregate is about 1.2 MB/s at four and rises slowly.
const DOWNLOAD_WORKERS: usize = 6;
/// How many galleries to pull from the popularity list per refill.
const REFILL_BATCH: usize = 200;
const SETTINGS_KEY: &str = "grinder";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GrinderSettings {
    /// Off by default. An app must not start hammering someone else's
    /// server the moment it is opened.
    pub enabled: bool,
    pub language: String,
    /// Gallery types to sweep, in order.
    pub kinds: Vec<String>,
    /// Download cap. Recognition only consumes about 2.8 MB/s, so a higher
    /// cap would just pile pages up.
    pub bytes_per_second: u64,
    /// Recognise galleries whose text was imported from elsewhere.
    ///
    /// An imported corpus has gaps: bubbles its OCR missed that Vision reads.
    /// Off by default because re-reading 108,000 galleries costs the same
    /// as indexing them from nothing.
    #[serde(default)]
    pub reindex_imported: bool,
    /// Recognise the pages of works the reader opens.
    ///
    /// On by default, unlike the sweep: those pages are already coming down
    /// the wire to be looked at, so this costs no download at all - only the
    /// recognition itself, which is what makes a work you have read findable
    /// by a line you remember.
    #[serde(default = "yes")]
    pub read_indexing: bool,
}

fn yes() -> bool {
    true
}

impl Default for GrinderSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            language: "korean".into(),
            kinds: vec!["doujinshi".into(), "manga".into()],
            bytes_per_second: 3 * 1024 * 1024,
            reindex_imported: false,
            read_indexing: true,
        }
    }
}

/// Progress of a bulk import from a published corpus.
#[derive(Debug, Default, Clone, Serialize)]
pub struct ImportProgress {
    pub running: bool,
    pub directory: String,
    pub chunks_total: usize,
    pub works_seen: usize,
    pub added: usize,
    pub skipped: usize,
    pub error: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct GrinderStatus {
    pub running: bool,
    pub current: Option<i32>,
    pub current_title: Option<String>,
    pub pages_per_second: f32,
    pub last_error: Option<String>,
    pub galleries_this_session: u64,
    pub pages_this_session: u64,
}

pub struct Grinder {
    fetcher: Arc<HttpFetcher>,
    cfg: Config,
    store: Arc<DialogueStore>,
    /// Rebuilt when the corpus language changes, so recognition is told
    /// which script to expect.
    ocr: RwLock<Arc<dyn Ocr>>,
    /// The language `ocr` was built for.
    ocr_language: RwLock<String>,
    /// How to build one; `None` where the platform has no recognition.
    make_ocr: fn(tsuburu_ocr::OcrOptions) -> Option<Box<dyn Ocr>>,
    /// Bounds recognition of pages the reader is looking at.
    read_permits: Arc<Semaphore>,
    settings: RwLock<GrinderSettings>,
    status: RwLock<GrinderStatus>,
    stop: AtomicBool,
    pages_done: AtomicU64,
    galleries_done: AtomicU64,
    /// Where the background sweep is in the popularity list.
    refill_cursor: RwLock<usize>,
    /// Updated from a blocking thread, hence a std mutex.
    import: std::sync::Mutex<Option<ImportProgress>>,
    /// The model tsuburu fetched and runs, when it is running. It answers on
    /// a port chosen at the time, so the address in the settings - which is
    /// there for a reader who runs their own - cannot name it.
    running_model: std::sync::RwLock<Option<Arc<tsuburu_embed::runner::Runner>>>,
}

impl Grinder {
    pub fn new(
        fetcher: Arc<HttpFetcher>,
        cfg: Config,
        store: Arc<DialogueStore>,
        ocr: Arc<dyn Ocr>,
    ) -> Self {
        Self::with_factory(fetcher, cfg, store, ocr, |_| None)
    }

    /// `make_ocr` is called when the corpus language changes.
    pub fn with_factory(
        fetcher: Arc<HttpFetcher>,
        cfg: Config,
        store: Arc<DialogueStore>,
        ocr: Arc<dyn Ocr>,
        make_ocr: fn(tsuburu_ocr::OcrOptions) -> Option<Box<dyn Ocr>>,
    ) -> Self {
        let settings: GrinderSettings = store
            .setting(SETTINGS_KEY)
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        Self {
            fetcher,
            cfg,
            store,
            read_permits: Arc::new(Semaphore::new(READ_WORKERS)),
            ocr_language: RwLock::new(settings.language.clone()),
            ocr: RwLock::new(ocr),
            make_ocr,
            settings: RwLock::new(settings),
            status: RwLock::new(GrinderStatus::default()),
            stop: AtomicBool::new(false),
            pages_done: AtomicU64::new(0),
            galleries_done: AtomicU64::new(0),
            refill_cursor: RwLock::new(0),
            import: std::sync::Mutex::new(None),
            running_model: std::sync::RwLock::new(None),
        }
    }

    pub fn import_progress(&self) -> Option<ImportProgress> {
        self.import.lock().ok().and_then(|g| g.clone())
    }

    /// Starts importing an `llm-search-index` directory in the
    /// background. Returns an error if one is already running.
    pub fn start_artifact_import(self: &Arc<Self>, dir: PathBuf) -> Result<(), String> {
        let reader = ChunkReader::open(&dir).map_err(|e| e.to_string())?;
        if let Err(err) = self.store.set_artifact_dir(&dir) {
            tracing::warn!(%err, "could not remember the artefact directory");
        }
        {
            let mut guard = self.import.lock().map_err(|e| e.to_string())?;
            if guard.as_ref().is_some_and(|p| p.running) {
                return Err("an import is already running".into());
            }
            *guard = Some(ImportProgress {
                running: true,
                directory: dir.display().to_string(),
                chunks_total: reader.len(),
                ..ImportProgress::default()
            });
        }

        let grinder = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            let store = grinder.store();
            let progress_ref = &grinder.import;
            let mut failed: Option<String> = None;
            let works = WorkGrouper::new(reader).filter_map(|item| match item {
                Ok(work) => Some(work),
                Err(err) => {
                    // One bad record must not lose the rest of the corpus.
                    tracing::warn!(%err, "skipping an unreadable record");
                    None
                }
            });
            let result = store.import_works(works, "artifact", 500, |seen| {
                if let Ok(mut guard) = progress_ref.lock()
                    && let Some(p) = guard.as_mut()
                {
                    p.works_seen = seen;
                }
            });
            match result {
                Ok(summary) => {
                    if let Ok(mut guard) = progress_ref.lock()
                        && let Some(p) = guard.as_mut()
                    {
                        p.added = summary.added;
                        p.skipped = summary.skipped;
                        p.works_seen = summary.galleries;
                    }
                }
                Err(err) => failed = Some(err.to_string()),
            }
            if let Ok(mut guard) = progress_ref.lock()
                && let Some(p) = guard.as_mut()
            {
                p.running = false;
                p.error = failed;
            }
        });
        Ok(())
    }

    pub async fn settings(&self) -> GrinderSettings {
        self.settings.read().await.clone()
    }

    /// Recognises one page the reader just looked at and files it.
    ///
    /// The bytes have already been fetched to be displayed, so nothing is
    /// downloaded here. Recognition is bounded by the same permit count as
    /// the sweep so that reading stays smooth.
    pub async fn recognise_read_page(
        self: &Arc<Self>,
        gallery: i32,
        page: u16,
        bytes: Vec<u8>,
        language: &str,
    ) {
        let Ok(permit) = Arc::clone(&self.read_permits).acquire_owned().await else { return };
        let ocr = self.ocr_for(language).await;
        let store = Arc::clone(&self.store);
        let recognised = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            ocr.recognize(&bytes).map(|mut lines| {
                reading_order(&mut lines);
                lines.into_iter().map(|l| l.text).collect::<Vec<_>>()
            })
        })
        .await;

        match recognised {
            Ok(Ok(lines)) if !lines.is_empty() => {
                let text = lines.join("\n");
                let pages = vec![PageText { page, lines }];
                match store.merge_pages(gallery, &pages, true) {
                    Ok(0) => {}
                    Ok(_) => self.embed_read_page(gallery, page, &text).await,
                    Err(err) => tracing::debug!(gallery, page, %err, "could not file a read page"),
                }
            }
            Ok(Ok(_)) => {}
            Ok(Err(err)) => {
                tracing::debug!(gallery, page, %err, "read page recognition failed");
                self.note_error(format!("page {} of {gallery}: {err}", page + 1)).await;
            }
            Err(err) => {
                tracing::debug!(gallery, page, %err, "read page task failed");
                self.note_error(format!("page {} of {gallery}: {err}", page + 1)).await;
            }
        }
    }

    /// Embeds a passage this machine read, if a pack is configured.
    ///
    /// An imported index covers its own corpus and stops there. A page read
    /// afterwards is scanned beside it, so meaning search reaches what you
    /// have actually read. Without a pack there is nothing to embed with and
    /// this quietly does nothing.
    /// Tells the sweep about the model the program is running itself.
    pub fn use_model(&self, model: Arc<tsuburu_embed::runner::Runner>) {
        if let Ok(mut held) = self.running_model.write() {
            *held = Some(model);
        }
    }

    /// Where to embed against: the model this program runs, if it is running,
    /// and otherwise whatever server the settings name.
    fn embedder_url(&self, saved: String) -> String {
        self.running_model
            .read()
            .ok()
            .and_then(|held| held.as_ref().and_then(|model| model.url()))
            .unwrap_or(saved)
    }

    async fn embed_read_page(&self, gallery: i32, page: u16, text: &str) {
        let Ok(Some(json)) = self.store.embedder() else { return };
        // The setting holds the address and the model; the width is the
        // index's, which is what the default carries.
        #[derive(serde::Deserialize)]
        struct Saved {
            url: String,
            model: String,
        }
        let Ok(saved) = serde_json::from_str::<Saved>(&json) else { return };
        let config = tsuburu_embed::embedder::EmbedderConfig {
            url: self.embedder_url(saved.url),
            model: saved.model,
            ..Default::default()
        };
        let body = tsuburu_embed::embedder::request_body(&config, text);
        let reply = match self.fetcher.post_json(&config.url, &body).await {
            Ok(reply) => reply,
            Err(err) => {
                tracing::debug!(gallery, page, %err, "no embedding server for a read page");
                return;
            }
        };
        match tsuburu_embed::embedder::parse_embedding(&reply, config.dims) {
            Ok(vector) => {
                if let Err(err) = self.store.put_vector(gallery, page, &vector) {
                    tracing::debug!(gallery, page, %err, "could not keep a read page's vector");
                }
            }
            Err(err) => tracing::debug!(gallery, page, %err, "read page could not be embedded"),
        }
    }

    /// Swaps in an engine for `language` if the current one is for another.
    async fn ocr_for(&self, language: &str) -> Arc<dyn Ocr> {
        if self.ocr_language.read().await.as_str() == language {
            return Arc::clone(&*self.ocr.read().await);
        }
        if let Some(engine) = (self.make_ocr)(tsuburu_ocr::OcrOptions::for_language(language)) {
            let engine: Arc<dyn Ocr> = Arc::from(engine);
            *self.ocr.write().await = Arc::clone(&engine);
            *self.ocr_language.write().await = language.to_string();
            tracing::info!(language, "switched text recognition");
            return engine;
        }
        Arc::clone(&*self.ocr.read().await)
    }

    pub async fn update_settings(&self, next: GrinderSettings) {
        if let Ok(json) = serde_json::to_string(&next)
            && let Err(err) = self.store.set_setting(SETTINGS_KEY, &json)
        {
            tracing::warn!(%err, "could not persist grinder settings");
        }
        *self.settings.write().await = next;
    }

    pub async fn status(&self) -> GrinderStatus {
        let mut status = self.status.read().await.clone();
        status.galleries_this_session = self.galleries_done.load(Ordering::Relaxed);
        status.pages_this_session = self.pages_done.load(Ordering::Relaxed);
        status
    }

    pub fn store(&self) -> &DialogueStore {
        &self.store
    }

    /// A handle of its own, for work that outlives the borrow - importing on
    /// a blocking thread, say.
    pub fn store_handle(&self) -> Arc<DialogueStore> {
        Arc::clone(&self.store)
    }

    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// Runs until shutdown. Idles cheaply while disabled.
    pub async fn run(self: Arc<Self>) {
        while !self.stop.load(Ordering::Relaxed) {
            if !self.settings.read().await.enabled {
                self.status.write().await.running = false;
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
            self.status.write().await.running = true;

            let next = match self.store.next_pending() {
                Ok(next) => next,
                Err(err) => {
                    self.note_error(format!("queue unavailable: {err}")).await;
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    continue;
                }
            };

            match next {
                Some(id) => self.process(id).await,
                None => match self.refill().await {
                    Ok(0) => tokio::time::sleep(Duration::from_secs(30)).await,
                    Ok(_) => {}
                    Err(err) => {
                        self.note_error(format!("could not refill the queue: {err}")).await;
                        tokio::time::sleep(Duration::from_secs(30)).await;
                    }
                },
            }
        }
        self.status.write().await.running = false;
    }

    /// Pulls the next slice of the popularity list that matches the
    /// configured types and is not already done.
    async fn refill(&self) -> Result<usize, String> {
        let settings = self.settings.read().await.clone();
        let language = settings.language.as_str();
        let popular = self.cfg.sort_list_url(Sort::PopularYear, language);
        let total =
            nozomi::count(self.fetcher.as_ref(), &popular).await.map_err(|e| e.to_string())?;

        // With re-indexing on, only what this machine has read counts as done.
        let done = if settings.reindex_imported {
            self.store.locally_indexed_ids().map_err(|e| e.to_string())?
        } else {
            self.store.done_ids().map_err(|e| e.to_string())?
        };
        let mut allowed: HashSet<i32> = HashSet::new();
        for kind in &settings.kinds {
            let url = self.cfg.type_list_url(kind, language);
            match nozomi::id_set(self.fetcher.as_ref(), &url).await {
                Ok(set) => allowed.extend(set),
                Err(err) => tracing::warn!(kind, %err, "type list unavailable"),
            }
        }

        let mut cursor = self.refill_cursor.write().await;
        let mut batch = Vec::new();
        while batch.len() < REFILL_BATCH && *cursor < total {
            let ids = nozomi::page(self.fetcher.as_ref(), &popular, *cursor, REFILL_BATCH)
                .await
                .map_err(|e| e.to_string())?;
            if ids.is_empty() {
                break;
            }
            *cursor += ids.len();
            batch.extend(ids.into_iter().filter(|id| allowed.contains(id) && !done.contains(id)));
        }
        if batch.is_empty() {
            return Ok(0);
        }
        self.store
            .enqueue_with(&batch, Priority::Background, settings.reindex_imported)
            .map_err(|e| e.to_string())
    }

    async fn note_error(&self, message: String) {
        tracing::warn!("{message}");
        self.status.write().await.last_error = Some(message);
    }

    /// Downloads and recognises one gallery, then commits it.
    pub async fn process(&self, id: i32) {
        let started = Instant::now();
        {
            let mut status = self.status.write().await;
            status.current = Some(id);
            status.current_title = None;
        }

        let result = self.process_inner(id).await;
        let elapsed = started.elapsed().as_secs_f32().max(0.001);

        match result {
            Ok(pages) => {
                let count = pages.len() as u64;
                if let Err(err) = self.store.complete(id, &pages) {
                    self.note_error(format!("could not store gallery {id}: {err}")).await;
                } else {
                    self.pages_done.fetch_add(count, Ordering::Relaxed);
                    self.galleries_done.fetch_add(1, Ordering::Relaxed);
                    let mut status = self.status.write().await;
                    let rate = count as f32 / elapsed;
                    status.pages_per_second = if status.pages_per_second == 0.0 {
                        rate
                    } else {
                        status.pages_per_second * 0.8 + rate * 0.2
                    };
                    status.last_error = None;
                }
            }
            Err(err) => {
                tracing::warn!(id, %err, "gallery failed");
                if let Err(store_err) = self.store.fail(id, &err) {
                    self.note_error(format!("could not record failure for {id}: {store_err}"))
                        .await;
                }
                self.status.write().await.last_error = Some(format!("gallery {id}: {err}"));
            }
        }

        self.status.write().await.current = None;
    }

    async fn process_inner(&self, id: i32) -> Result<Vec<PageText>, String> {
        let started = Instant::now();
        let gallery = tsuburu_hitomi::fetch_gallery(self.fetcher.as_ref(), &self.cfg, id)
            .await
            .map_err(|e| e.to_string())?;
        self.status.write().await.current_title = gallery.title.clone();
        let gg = tsuburu_hitomi::fetch_gg(self.fetcher.as_ref(), &self.cfg)
            .await
            .map_err(|e| e.to_string())?;
        tracing::debug!(
            id,
            ms = started.elapsed().as_millis() as u64,
            pages = gallery.files.len(),
            "metadata ready"
        );

        let urls: Vec<(u16, String)> = gallery
            .files
            .iter()
            .enumerate()
            .filter_map(|(i, file)| {
                tsuburu_hitomi::image_url(&self.cfg, &gg, file).ok().map(|u| (i as u16, u))
            })
            .collect();

        // Downloader feeds a bounded channel; recognition drains it on
        // blocking threads. Both sides overlap.
        let (tx, mut rx) = mpsc::channel::<(u16, Vec<u8>)>(PIPELINE_DEPTH);
        let fetcher = Arc::clone(&self.fetcher);
        let cap = self.settings.read().await.bytes_per_second.max(64 * 1024);
        let downloader = tokio::spawn(async move {
            let started = Instant::now();
            let mut bytes_total = 0u64;
            let mut inflight = tokio::task::JoinSet::new();
            let mut remaining = urls.into_iter();
            loop {
                while inflight.len() < DOWNLOAD_WORKERS {
                    let Some((page, url)) = remaining.next() else { break };
                    let fetcher = Arc::clone(&fetcher);
                    inflight.spawn(async move { (page, fetcher.get(&url).await) });
                }
                let Some(joined) = inflight.join_next().await else {
                    tracing::debug!(
                        ms = started.elapsed().as_millis() as u64,
                        bytes = bytes_total,
                        "downloads finished"
                    );
                    break;
                };
                let body = match joined {
                    Ok((page, Ok(body))) => (page, body),
                    Ok((page, Err(err))) => {
                        tracing::debug!(page, %err, "page download failed; skipping");
                        continue;
                    }
                    Err(err) => {
                        tracing::debug!(%err, "download task panicked");
                        continue;
                    }
                };
                bytes_total += body.1.len() as u64;
                if tx.send(body).await.is_err() {
                    break;
                }
                // Stay under the cap by holding back the next dispatch.
                let budget = Duration::from_secs_f64(bytes_total as f64 / cap as f64);
                if let Some(wait) = budget.checked_sub(started.elapsed()) {
                    tokio::time::sleep(wait).await;
                }
            }
        });

        let language = self.settings.read().await.language.clone();
        let ocr = self.ocr_for(&language).await;
        let pages = self.recognise_with(&mut rx, ocr).await;
        downloader.await.map_err(|e| e.to_string())?;
        tracing::debug!(id, ms = started.elapsed().as_millis() as u64, "gallery recognised");
        pages
    }

    /// Recognises pages as they arrive. Public so the pipeline can be
    /// exercised with in-memory bytes and a mock OCR.
    pub async fn recognise(
        &self,
        rx: &mut mpsc::Receiver<(u16, Vec<u8>)>,
    ) -> Result<Vec<PageText>, String> {
        let ocr = Arc::clone(&*self.ocr.read().await);
        self.recognise_with(rx, ocr).await
    }

    async fn recognise_with(
        &self,
        rx: &mut mpsc::Receiver<(u16, Vec<u8>)>,
        ocr: Arc<dyn Ocr>,
    ) -> Result<Vec<PageText>, String> {
        let limiter = Arc::new(Semaphore::new(OCR_WORKERS));
        let mut tasks = tokio::task::JoinSet::new();
        while let Some((page, bytes)) = rx.recv().await {
            let permit = Arc::clone(&limiter).acquire_owned().await.map_err(|e| e.to_string())?;
            let ocr = Arc::clone(&ocr);
            tasks.spawn_blocking(move || {
                let _permit = permit;
                let text = ocr.recognize(&bytes).map(|mut lines| {
                    reading_order(&mut lines);
                    lines.into_iter().map(|l| l.text).collect::<Vec<_>>()
                });
                (page, text)
            });
        }

        let ocr_started = Instant::now();
        let mut pages = Vec::new();
        let mut failures = 0usize;
        let mut why: Option<String> = None;
        while let Some(joined) = tasks.join_next().await {
            match joined {
                Ok((page, Ok(lines))) => pages.push(PageText { page, lines }),
                Ok((page, Err(err))) => {
                    failures += 1;
                    why.get_or_insert_with(|| err.to_string());
                    tracing::debug!(page, %err, "page recognition failed");
                }
                Err(err) => {
                    failures += 1;
                    why.get_or_insert_with(|| err.to_string());
                    tracing::debug!(%err, "recognition task panicked");
                }
            }
        }
        tracing::debug!(
            ms = ocr_started.elapsed().as_millis() as u64,
            pages = pages.len(),
            failures,
            "recognition drained"
        );
        if pages.is_empty() {
            return Err(match why {
                Some(why) => format!("no page could be recognised ({failures} failures): {why}"),
                None => format!("no page could be recognised ({failures} failures)"),
            });
        }
        pages.sort_by_key(|p| p.page);
        Ok(pages)
    }

    /// Fraction of the most popular galleries that are indexed, so the
    /// UI can say "top 12% covered" rather than a raw count.
    pub async fn coverage(&self) -> Result<Coverage, String> {
        let settings = self.settings.read().await.clone();
        let language = settings.language.as_str();
        let popular = self.cfg.sort_list_url(Sort::PopularYear, language);
        // With re-indexing on, only what this machine has read counts as done.
        let done = if settings.reindex_imported {
            self.store.locally_indexed_ids().map_err(|e| e.to_string())?
        } else {
            self.store.done_ids().map_err(|e| e.to_string())?
        };
        let top = nozomi::page(self.fetcher.as_ref(), &popular, 0, 10_000)
            .await
            .map_err(|e| e.to_string())?;

        let covered = |n: usize| top.iter().take(n).filter(|id| done.contains(id)).count();
        let mut total = 0usize;
        for kind in &settings.kinds {
            let url = self.cfg.type_list_url(kind, language);
            total += nozomi::count(self.fetcher.as_ref(), &url).await.unwrap_or(0);
        }
        // An imported corpus can cover types outside the sweep's own list,
        // so the total is at least what is already done.
        Ok(Coverage {
            top_1k: covered(1_000),
            top_10k: covered(10_000.min(top.len())),
            top_10k_total: 10_000.min(top.len()),
            done: done.len(),
            total: total.max(done.len()),
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Coverage {
    pub top_1k: usize,
    pub top_10k: usize,
    pub top_10k_total: usize,
    pub done: usize,
    pub total: usize,
}

/// Pulls gallery ids out of pasted text: bare numbers or hitomi URLs
/// such as `https://hitomi.la/doujinshi/title-korean-1234567.html`.
pub fn ids_from_text(text: &str) -> Vec<i32> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for token in text.split(|c: char| c.is_whitespace() || c == ',' || c == ';') {
        let candidate =
            token.trim().trim_end_matches(".html").rsplit(['-', '/']).next().unwrap_or("");
        if let Ok(id) = candidate.parse::<i32>()
            && id > 0
            && seen.insert(id)
        {
            out.push(id);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_ids_from_urls_and_numbers() {
        let text = "https://hitomi.la/doujinshi/some-title-korean-1234567.html\n\
                    hitomi.la/reader/2222222.html, 3333333 ; 3333333 junk";
        assert_eq!(ids_from_text(text), vec![1234567, 2222222, 3333333]);
    }

    #[test]
    fn default_settings_are_off() {
        assert!(!GrinderSettings::default().enabled);
    }
}
