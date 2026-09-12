//! Fetching the model that turns a phrase into a vector, and running it.
//!
//! The index was built with Qwen3-Embedding-4B, so nothing else can be asked
//! about it: a different model lands in a different space and the numbers mean
//! nothing. Several gigabytes of weights are not something to ship inside a
//! four megabyte program — but asking a reader to install a model server, find
//! a file of weights and paste an address is not something to ask either.
//!
//! So nothing is fetched until the feature is switched on, and then tsuburu
//! fetches both halves itself: the weights from the people who trained them,
//! and a server to run them from the people who wrote it. Switching it off
//! stops the server; the files stay, so switching it on again is immediate.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};

/// The llama.cpp build to run. Pinned: a release that is known to work is
/// worth more here than the newest one, and it is what the download resumes
/// against.
const LLAMA: &str = "b10924";
/// Plain CPU builds. The others want a driver stack that may not be there,
/// and on Apple silicon this one uses the GPU anyway.
const LLAMA_URL: &str = "https://github.com/ggml-org/llama.cpp/releases/download";

/// Qwen's own publication of the weights, Apache-2.0, no sign-in.
const MODEL_URL: &str = "https://huggingface.co/Qwen/Qwen3-Embedding-4B-GGUF/resolve/main/Qwen3-Embedding-4B-Q4_K_M.gguf";
const MODEL_FILE: &str = "Qwen3-Embedding-4B-Q4_K_M.gguf";
/// What the weights weigh, so there is something to show progress against
/// before the first byte arrives.
const MODEL_BYTES: u64 = 2_496_703_776;

/// How long to wait for the server to load the weights and answer.
const READY_TRIES: usize = 600;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum Progress {
    /// Nothing fetched, nothing running.
    Off,
    /// Getting one of the two halves.
    Fetching {
        what: String,
        done: u64,
        total: u64,
    },
    /// Both are here; the weights are being read.
    Starting,
    Ready,
    Failed {
        error: String,
    },
}

impl Progress {
    pub fn is_busy(&self) -> bool {
        matches!(self, Progress::Fetching { .. } | Progress::Starting)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("no llama.cpp build is published for this computer")]
    Unsupported,
    #[error("could not fetch {0}: {1}")]
    Fetch(String, String),
    #[error("could not unpack {0}: {1}")]
    Unpack(String, String),
    #[error("could not start the model: {0}")]
    Start(String),
    #[error("the model did not answer in time")]
    Silent,
}

/// Owns the files, the child process and what to tell the interface.
pub struct Runner {
    dir: PathBuf,
    progress: Mutex<Progress>,
    child: Mutex<Option<Child>>,
    port: Mutex<Option<u16>>,
    client: reqwest::Client,
}

impl Runner {
    pub fn new(dir: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            dir,
            progress: Mutex::new(Progress::Off),
            child: Mutex::new(None),
            port: Mutex::new(None),
            client: reqwest::Client::builder()
                .user_agent(concat!("tsuburu/", env!("CARGO_PKG_VERSION")))
                .build()
                .unwrap_or_default(),
        })
    }

    pub fn progress(&self) -> Progress {
        self.progress.lock().map(|p| p.clone()).unwrap_or(Progress::Off)
    }

    /// The endpoint to embed against, once there is one.
    pub fn url(&self) -> Option<String> {
        let port = (*self.port.lock().ok()?)?;
        matches!(self.progress(), Progress::Ready)
            .then(|| format!("http://127.0.0.1:{port}/v1/embeddings"))
    }

    /// Whether a reader has asked for this, which outlives the process: it was
    /// switched on, so it should be on again next time without being asked.
    pub fn wanted(&self) -> bool {
        self.dir.join("wanted").is_file()
    }

    /// Brings the model back up if it was left on and its files are here.
    /// Nothing is fetched: this is only for a switch that was already thrown.
    pub fn resume(self: &Arc<Self>) {
        if self.wanted() && self.kept() {
            self.enable();
        }
    }

    /// Whether both halves are already on disk, so switching on costs nothing
    /// but the time to read the weights.
    pub fn kept(&self) -> bool {
        self.model_path().is_file() && self.server_path().is_some_and(|p| p.is_file())
    }

    /// What is on disk, whether or not it is running.
    pub fn bytes_kept(&self) -> u64 {
        std::fs::metadata(self.model_path()).map(|m| m.len()).unwrap_or(0)
    }

    fn model_path(&self) -> PathBuf {
        self.dir.join(MODEL_FILE)
    }

    /// Everything unpacked from one release, under one name.
    fn runtime_dir(&self) -> PathBuf {
        self.dir.join(format!("llama-{LLAMA}"))
    }

    fn server_path(&self) -> Option<PathBuf> {
        find_server(&self.runtime_dir())
    }

    pub fn enable(self: &Arc<Self>) {
        if self.progress().is_busy() || self.progress() == Progress::Ready {
            return;
        }
        let _ = std::fs::create_dir_all(&self.dir);
        let _ = std::fs::write(self.dir.join("wanted"), b"");
        let me = Arc::clone(self);
        tokio::spawn(async move {
            if let Err(error) = me.bring_up().await {
                tracing::warn!(%error, "the meaning model could not be brought up");
                me.set(Progress::Failed { error: error.to_string() });
            }
        });
    }

    pub fn disable(&self) {
        let _ = std::fs::remove_file(self.dir.join("wanted"));
        if let Ok(mut held) = self.child.lock()
            && let Some(mut child) = held.take()
        {
            let _ = child.start_kill();
        }
        if let Ok(mut port) = self.port.lock() {
            *port = None;
        }
        self.set(Progress::Off);
    }

    fn set(&self, next: Progress) {
        if let Ok(mut held) = self.progress.lock() {
            *held = next;
        }
    }

    async fn bring_up(self: &Arc<Self>) -> Result<(), RunError> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| RunError::Fetch(self.dir.display().to_string(), e.to_string()))?;

        let server = match self.server_path() {
            Some(path) if path.is_file() => path,
            _ => self.fetch_server().await?,
        };
        if !self.model_path().is_file() {
            self.fetch("model", MODEL_URL, &self.model_path(), MODEL_BYTES).await?;
        }

        self.set(Progress::Starting);
        self.start(&server).await
    }

    /// The llama.cpp build for this computer, unpacked beside the weights.
    async fn fetch_server(self: &Arc<Self>) -> Result<PathBuf, RunError> {
        let asset = asset_name().ok_or(RunError::Unsupported)?;
        let url = format!("{LLAMA_URL}/{LLAMA}/{asset}");
        let archive = self.dir.join(&asset);
        self.fetch("server", &url, &archive, 0).await?;

        // Both tarballs and zips: every platform tsuburu runs on has a tar
        // that reads either, which is a great deal less than a decompressor
        // of our own for each.
        //
        // Into a directory of our own, because the archives do not agree on
        // whether they have one: the tarballs hold a llama-<tag>/ folder and
        // the Windows zip is flat, so unpacking where it stands would spray
        // thirty DLLs beside the weights.
        let into = self.runtime_dir();
        std::fs::create_dir_all(&into)
            .map_err(|e| RunError::Unpack(asset.clone(), e.to_string()))?;
        let status = Command::new("tar")
            .arg("-xf")
            .arg(&archive)
            .arg("-C")
            .arg(&into)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map_err(|e| RunError::Unpack(asset.clone(), e.to_string()))?;
        if !status.success() {
            return Err(RunError::Unpack(asset, format!("tar exited {status}")));
        }
        let _ = std::fs::remove_file(&archive);

        // Found rather than guessed at, for the same reason.
        let server = find_server(&into)
            .ok_or_else(|| RunError::Unpack(asset, "no llama-server inside".into()))?;
        make_runnable(&server);
        Ok(server)
    }

    /// Streams a file to disk, resuming whatever a previous attempt left.
    async fn fetch(
        self: &Arc<Self>,
        what: &str,
        url: &str,
        to: &Path,
        expected: u64,
    ) -> Result<(), RunError> {
        let part = to.with_extension("part");
        let have = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);

        let mut request = self.client.get(url);
        if have > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let response =
            request.send().await.map_err(|e| RunError::Fetch(what.into(), e.to_string()))?;
        let resuming = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        if !response.status().is_success() {
            return Err(RunError::Fetch(what.into(), format!("answered {}", response.status())));
        }

        let mut done = if resuming { have } else { 0 };
        let total = response.content_length().map(|len| done + len).unwrap_or(expected).max(done);
        self.set(Progress::Fetching { what: what.into(), done, total });

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(resuming)
            .truncate(!resuming)
            .open(&part)
            .await
            .map_err(|e| RunError::Fetch(what.into(), e.to_string()))?;

        let mut stream = response.bytes_stream();
        let mut since = 0u64;
        while let Some(chunk) = futures_util::StreamExt::next(&mut stream).await {
            let chunk = chunk.map_err(|e| RunError::Fetch(what.into(), e.to_string()))?;
            file.write_all(&chunk)
                .await
                .map_err(|e| RunError::Fetch(what.into(), e.to_string()))?;
            done += chunk.len() as u64;
            since += chunk.len() as u64;
            // Telling the interface about every chunk of a 2 GB file is a lot
            // of locking for a bar that moves in pixels.
            if since > 4 << 20 {
                since = 0;
                self.set(Progress::Fetching { what: what.into(), done, total: total.max(done) });
            }
        }
        file.flush().await.map_err(|e| RunError::Fetch(what.into(), e.to_string()))?;
        drop(file);
        std::fs::rename(&part, to).map_err(|e| RunError::Fetch(what.into(), e.to_string()))?;
        Ok(())
    }

    async fn start(self: &Arc<Self>, server: &Path) -> Result<(), RunError> {
        let port = free_port().ok_or_else(|| RunError::Start("no free port".into()))?;
        let child = Command::new(server)
            .arg("-m")
            .arg(self.model_path())
            .args(["--host", "127.0.0.1"])
            .args(["--port", &port.to_string()])
            // Embeddings only, and Qwen3 pools the last token rather than the
            // mean; the wrong pooling gives numbers that look fine and are not.
            .arg("--embeddings")
            .args(["--pooling", "last"])
            .args(["-c", "2048"])
            .args(["-ub", "2048"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| RunError::Start(e.to_string()))?;

        if let Ok(mut held) = self.child.lock()
            && let Some(mut old) = held.replace(child)
        {
            let _ = old.start_kill();
        }
        if let Ok(mut held) = self.port.lock() {
            *held = Some(port);
        }

        // Reading four billion parameters off a disk takes a while, and how
        // long depends on the disk.
        let health = format!("http://127.0.0.1:{port}/health");
        for _ in 0..READY_TRIES {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            if let Ok(mut held) = self.child.lock()
                && let Some(child) = held.as_mut()
                && matches!(child.try_wait(), Ok(Some(_)))
            {
                return Err(RunError::Start("the model server stopped".into()));
            }
            if self.client.get(&health).send().await.is_ok_and(|r| r.status().is_success()) {
                self.set(Progress::Ready);
                return Ok(());
            }
        }
        self.disable();
        Err(RunError::Silent)
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        if let Ok(mut held) = self.child.lock()
            && let Some(mut child) = held.take()
        {
            let _ = child.start_kill();
        }
    }
}

/// The model server, wherever this platform's archive happened to put it.
///
/// The tarballs unpack into a folder named for the release and the Windows zip
/// unpacks flat, so the only thing worth relying on is the name of the file.
fn find_server(dir: &Path) -> Option<PathBuf> {
    let wanted = if cfg!(windows) { "llama-server.exe" } else { "llama-server" };
    let mut look = vec![dir.to_path_buf()];
    while let Some(at) = look.pop() {
        let Ok(entries) = std::fs::read_dir(&at) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                look.push(path);
            } else if path.file_name().is_some_and(|name| name == wanted) {
                return Some(path);
            }
        }
    }
    None
}

/// Which published build belongs to a computer. Plain CPU builds throughout:
/// the others want a driver stack that may not be there, and on Apple silicon
/// this one uses the GPU anyway.
fn asset_for(os: &str, arch: &str) -> Option<String> {
    let platform = match (os, arch) {
        ("macos", "aarch64") => "macos-arm64.tar.gz",
        ("macos", "x86_64") => "macos-x64.tar.gz",
        ("linux", "x86_64") => "ubuntu-x64.tar.gz",
        ("linux", "aarch64") => "ubuntu-arm64.tar.gz",
        ("windows", "x86_64") => "win-cpu-x64.zip",
        ("windows", "aarch64") => "win-cpu-arm64.zip",
        _ => return None,
    };
    Some(format!("llama-{LLAMA}-bin-{platform}"))
}

fn asset_name() -> Option<String> {
    asset_for(std::env::consts::OS, std::env::consts::ARCH)
}

fn free_port() -> Option<u16> {
    std::net::TcpListener::bind("127.0.0.1:0").ok()?.local_addr().ok().map(|a| a.port())
}

#[cfg(unix)]
fn make_runnable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755));
}

#[cfg(not(unix))]
fn make_runnable(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_platform_the_release_builds_for_has_a_model_server() {
        // The four targets the release workflow produces, by name, so a
        // rename upstream is caught here rather than by a reader.
        assert_eq!(
            asset_for("macos", "aarch64").unwrap(),
            format!("llama-{LLAMA}-bin-macos-arm64.tar.gz")
        );
        assert_eq!(
            asset_for("macos", "x86_64").unwrap(),
            format!("llama-{LLAMA}-bin-macos-x64.tar.gz")
        );
        assert_eq!(
            asset_for("linux", "x86_64").unwrap(),
            format!("llama-{LLAMA}-bin-ubuntu-x64.tar.gz")
        );
        assert_eq!(
            asset_for("windows", "x86_64").unwrap(),
            format!("llama-{LLAMA}-bin-win-cpu-x64.zip")
        );
        assert!(asset_name().is_some(), "no build for this computer");
    }

    #[test]
    fn a_computer_nobody_publishes_for_is_said_so_rather_than_guessed_at() {
        assert!(asset_for("freebsd", "x86_64").is_none());
        assert!(asset_for("linux", "mips").is_none());
    }

    #[test]
    fn nothing_is_running_before_it_is_switched_on() {
        let dir = tempfile::tempdir().unwrap();
        let runner = Runner::new(dir.path().to_path_buf());
        assert_eq!(runner.progress(), Progress::Off);
        assert!(runner.url().is_none());
        assert!(!runner.kept());
        assert_eq!(runner.bytes_kept(), 0);
    }

    #[test]
    fn being_switched_on_outlives_the_program() {
        let dir = tempfile::tempdir().unwrap();
        let runner = Runner::new(dir.path().to_path_buf());
        assert!(!runner.wanted());

        std::fs::write(dir.path().join("wanted"), b"").unwrap();
        assert!(runner.wanted());
        // But not enough on its own: without the files there is nothing to
        // bring up, and resuming must not start a download nobody asked for.
        assert!(!runner.kept());
        runner.resume();
        assert_eq!(runner.progress(), Progress::Off);

        runner.disable();
        assert!(!runner.wanted());
    }

    /// The three archives do not agree on where they put it, and the reader
    /// on the platform whose archive is flat is the one who found out.
    #[test]
    fn the_server_is_found_whether_the_archive_had_a_folder_or_not() {
        let exe = if cfg!(windows) { "llama-server.exe" } else { "llama-server" };

        // The Windows zip: everything at the top, no folder at all.
        let flat = tempfile::tempdir().unwrap();
        std::fs::write(flat.path().join("ggml-base.dll"), b"").unwrap();
        std::fs::write(flat.path().join(exe), b"").unwrap();
        assert_eq!(find_server(flat.path()), Some(flat.path().join(exe)));

        // The tarballs: one folder named for the release.
        let nested = tempfile::tempdir().unwrap();
        let inside = nested.path().join("llama-b10924");
        std::fs::create_dir_all(&inside).unwrap();
        std::fs::write(inside.join("libggml.dylib"), b"").unwrap();
        std::fs::write(inside.join(exe), b"").unwrap();
        assert_eq!(find_server(nested.path()), Some(inside.join(exe)));
    }

    #[test]
    fn an_archive_without_one_is_not_mistaken_for_one_with() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("llama-cli"), b"").unwrap();
        std::fs::write(dir.path().join("llama-server-impl.dll"), b"").unwrap();
        assert_eq!(find_server(dir.path()), None);
    }

    #[test]
    fn a_free_port_is_free() {
        let port = free_port().unwrap();
        assert!(port > 0);
        assert!(std::net::TcpListener::bind(("127.0.0.1", port)).is_ok());
    }
}
