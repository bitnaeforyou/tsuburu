//! Noticing that a newer tsuburu exists, and becoming it.
//!
//! The program is one file that somebody downloaded, so the only way it ever
//! gets fixed is if it says so and then does it. It asks where it came from -
//! not where this source was written, which is a different place and none of
//! a reader's business - because the release that built it is the release that
//! will have the next one.
//!
//! Nothing is checked, downloaded or replaced unless the reader says so; the
//! only thing that happens on its own is the asking, and that can be turned
//! off.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

/// Where the binary that is running was published. Baked in by the workflow
/// that built it; a build from a checkout has none and never offers updates.
const RELEASES: Option<&str> = option_env!("TSUBURU_RELEASES");

pub const HERE: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum Progress {
    /// Nothing known yet, or nothing newer than this.
    Idle,
    Checking,
    /// There is a newer one, and this is what it is called.
    Found {
        version: String,
        notes: String,
    },
    Fetching {
        done: u64,
        total: u64,
    },
    /// Replaced. The reader has to start it again.
    Ready {
        version: String,
    },
    Failed {
        error: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("this copy was not published, so there is nothing to update from")]
    Unpublished,
    #[error("no build of {0} for this computer")]
    NoAsset(String),
    #[error("could not reach the releases: {0}")]
    Unreachable(String),
    #[error("could not unpack it: {0}")]
    Unpack(String),
    #[error("could not put it in place: {0}")]
    Replace(String),
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize, Clone)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// Whether `there` is a later version than `here`.
///
/// Only the three numbers are compared. A tag that is not three numbers is
/// not something to act on, so it is not newer.
pub fn newer(here: &str, there: &str) -> bool {
    match (numbers(here), numbers(there)) {
        (Some(a), Some(b)) => b > a,
        _ => false,
    }
}

fn numbers(version: &str) -> Option<[u32; 3]> {
    let mut parts = version.trim().trim_start_matches('v').split('.');
    let mut out = [0u32; 3];
    for slot in &mut out {
        *slot = parts.next()?.parse().ok()?;
    }
    parts.next().is_none().then_some(out)
}

/// The published file for this computer, by the name the workflow gives it.
pub fn asset_for(os: &str, arch: &str) -> Option<&'static str> {
    Some(match (os, arch) {
        ("macos", "aarch64") => "tsuburu-aarch64-apple-darwin.zip",
        ("macos", "x86_64") => "tsuburu-x86_64-apple-darwin.zip",
        ("linux", "x86_64") => "tsuburu-x86_64-unknown-linux-gnu.tar.gz",
        ("windows", "x86_64") => "tsuburu-x86_64-pc-windows-msvc.zip",
        _ => return None,
    })
}

pub struct Updater {
    progress: Mutex<Progress>,
    client: reqwest::Client,
}

impl Updater {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            progress: Mutex::new(Progress::Idle),
            client: reqwest::Client::builder()
                .user_agent(concat!("tsuburu/", env!("CARGO_PKG_VERSION")))
                .build()
                .unwrap_or_default(),
        })
    }

    /// Whether this copy knows where it was published from at all.
    pub fn publishable(&self) -> bool {
        RELEASES.is_some()
    }

    pub fn progress(&self) -> Progress {
        self.progress.lock().map(|p| p.clone()).unwrap_or(Progress::Idle)
    }

    fn set(&self, next: Progress) {
        if let Ok(mut held) = self.progress.lock() {
            *held = next;
        }
    }

    /// Asks once whether there is a newer one. Says nothing if there is not.
    pub fn check(self: &Arc<Self>) {
        if matches!(self.progress(), Progress::Checking | Progress::Fetching { .. }) {
            return;
        }
        let me = Arc::clone(self);
        tokio::spawn(async move {
            me.set(Progress::Checking);
            match me.latest().await {
                Ok(Some(release)) => me.set(Progress::Found {
                    version: release.tag_name.trim_start_matches('v').to_string(),
                    notes: first_lines(&release.body),
                }),
                Ok(None) => me.set(Progress::Idle),
                Err(error) => {
                    tracing::debug!(%error, "could not look for an update");
                    me.set(Progress::Idle)
                }
            }
        });
    }

    async fn latest(&self) -> Result<Option<Release>, UpdateError> {
        let repo = RELEASES.ok_or(UpdateError::Unpublished)?;
        let url = format!("https://api.github.com/repos/{repo}/releases/latest");
        let release: Release = self
            .client
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| UpdateError::Unreachable(e.to_string()))?
            .error_for_status()
            .map_err(|e| UpdateError::Unreachable(e.to_string()))?
            .json()
            .await
            .map_err(|e| UpdateError::Unreachable(e.to_string()))?;

        Ok(newer(HERE, &release.tag_name).then_some(release))
    }

    /// Fetches the newer one and puts it where this one is.
    ///
    /// The running program is not overwritten in place: it is moved aside
    /// first, which every platform allows and which leaves something to go
    /// back to if the write fails halfway.
    pub fn apply(self: &Arc<Self>) {
        if matches!(self.progress(), Progress::Fetching { .. } | Progress::Ready { .. }) {
            return;
        }
        let me = Arc::clone(self);
        tokio::spawn(async move {
            match me.replace_self().await {
                Ok(version) => me.set(Progress::Ready { version }),
                Err(error) => {
                    tracing::warn!(%error, "the update could not be applied");
                    me.set(Progress::Failed { error: error.to_string() })
                }
            }
        });
    }

    async fn replace_self(self: &Arc<Self>) -> Result<String, UpdateError> {
        let release = self.latest().await?.ok_or(UpdateError::Unpublished)?;
        let want = asset_for(std::env::consts::OS, std::env::consts::ARCH)
            .ok_or_else(|| UpdateError::NoAsset(release.tag_name.clone()))?;
        let asset = release
            .assets
            .iter()
            .find(|a| a.name == want)
            .ok_or_else(|| UpdateError::NoAsset(want.to_string()))?
            .clone();

        let here = std::env::current_exe().map_err(|e| UpdateError::Replace(e.to_string()))?;
        let work = here
            .parent()
            .ok_or_else(|| UpdateError::Replace("nowhere to write".into()))?
            .join(".tsuburu-update");
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(&work).map_err(|e| UpdateError::Replace(e.to_string()))?;

        let archive = work.join(&asset.name);
        self.fetch(&asset.browser_download_url, &archive).await?;

        let status = tokio::process::Command::new("tar")
            .arg("-xf")
            .arg(&archive)
            .current_dir(&work)
            .status()
            .await
            .map_err(|e| UpdateError::Unpack(e.to_string()))?;
        if !status.success() {
            return Err(UpdateError::Unpack(format!("tar exited {status}")));
        }

        let fresh =
            find_binary(&work).ok_or_else(|| UpdateError::Unpack("no tsuburu inside".into()))?;
        swap(&here, &fresh)?;
        let _ = std::fs::remove_dir_all(&work);
        Ok(release.tag_name.trim_start_matches('v').to_string())
    }

    async fn fetch(self: &Arc<Self>, url: &str, to: &Path) -> Result<(), UpdateError> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| UpdateError::Unreachable(e.to_string()))?
            .error_for_status()
            .map_err(|e| UpdateError::Unreachable(e.to_string()))?;

        let total = response.content_length().unwrap_or(0);
        let mut done = 0u64;
        self.set(Progress::Fetching { done, total });

        let mut file =
            tokio::fs::File::create(to).await.map_err(|e| UpdateError::Replace(e.to_string()))?;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = futures_util::StreamExt::next(&mut stream).await {
            let chunk = chunk.map_err(|e| UpdateError::Unreachable(e.to_string()))?;
            file.write_all(&chunk).await.map_err(|e| UpdateError::Replace(e.to_string()))?;
            done += chunk.len() as u64;
            self.set(Progress::Fetching { done, total: total.max(done) });
        }
        file.flush().await.map_err(|e| UpdateError::Replace(e.to_string()))?;
        Ok(())
    }
}

/// The one file in the unpacked archive that is the program.
fn find_binary(dir: &Path) -> Option<PathBuf> {
    let wanted = if cfg!(windows) { "tsuburu.exe" } else { "tsuburu" };
    let mut stack = vec![dir.to_path_buf()];
    while let Some(at) = stack.pop() {
        for entry in std::fs::read_dir(&at).ok()?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().is_some_and(|n| n == wanted) {
                return Some(path);
            }
        }
    }
    None
}

/// Puts `fresh` where `here` is, keeping the old one until the new one lands.
fn swap(here: &Path, fresh: &Path) -> Result<(), UpdateError> {
    let aside = here.with_extension("old");
    let _ = std::fs::remove_file(&aside);
    // Renaming a running program is allowed everywhere; overwriting one is
    // not allowed on Windows at all.
    std::fs::rename(here, &aside).map_err(|e| UpdateError::Replace(e.to_string()))?;
    if let Err(error) = std::fs::copy(fresh, here) {
        let _ = std::fs::rename(&aside, here);
        return Err(UpdateError::Replace(error.to_string()));
    }
    runnable(here);
    // On Unix the old one goes now; on Windows it is still open, and is swept
    // up the next time the program starts.
    #[cfg(unix)]
    let _ = std::fs::remove_file(&aside);
    Ok(())
}

/// Removes what a previous update left behind, if anything did.
pub fn sweep() {
    if let Ok(here) = std::env::current_exe() {
        let _ = std::fs::remove_file(here.with_extension("old"));
    }
}

#[cfg(unix)]
fn runnable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755));
}

#[cfg(not(unix))]
fn runnable(_path: &Path) {}

fn first_lines(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('|') && !line.starts_with('<'))
        .take(3)
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(240)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_numbers_are_newer() {
        assert!(newer("0.2.0", "0.2.1"));
        assert!(newer("0.2.0", "v0.3.0"));
        assert!(newer("0.9.9", "1.0.0"));
        assert!(!newer("0.2.0", "0.2.0"));
        assert!(!newer("0.3.0", "0.2.9"));
    }

    #[test]
    fn a_tag_that_is_not_a_version_is_not_an_update() {
        assert!(!newer("0.2.0", "nightly"));
        assert!(!newer("0.2.0", "v1.0"));
        assert!(!newer("0.2.0", "1.0.0.1"));
        assert!(!newer("", "1.0.0"));
    }

    #[test]
    fn every_platform_the_release_builds_for_has_a_file() {
        assert_eq!(asset_for("macos", "aarch64"), Some("tsuburu-aarch64-apple-darwin.zip"));
        assert_eq!(asset_for("macos", "x86_64"), Some("tsuburu-x86_64-apple-darwin.zip"));
        assert_eq!(asset_for("linux", "x86_64"), Some("tsuburu-x86_64-unknown-linux-gnu.tar.gz"));
        assert_eq!(asset_for("windows", "x86_64"), Some("tsuburu-x86_64-pc-windows-msvc.zip"));
        assert_eq!(asset_for("freebsd", "x86_64"), None);
    }

    #[test]
    fn a_copy_built_from_a_checkout_offers_nothing() {
        // No workflow set the variable, so this test binary has none either.
        assert!(!Updater::new().publishable());
        assert_eq!(Updater::new().progress(), Progress::Idle);
    }

    #[test]
    fn the_old_one_is_kept_until_the_new_one_is_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let here = dir.path().join("tsuburu");
        let fresh = dir.path().join("fresh");
        std::fs::write(&here, b"old").unwrap();
        std::fs::write(&fresh, b"new").unwrap();

        swap(&here, &fresh).unwrap();
        assert_eq!(std::fs::read(&here).unwrap(), b"new");
    }

    #[test]
    fn release_notes_are_trimmed_to_something_a_line_can_hold() {
        let body = "\n\n| a | table |\n<img src=x>\nFirst line.\nSecond line.\nThird.\nFourth.";
        let notes = first_lines(body);
        assert!(notes.starts_with("First line."), "{notes}");
        assert!(!notes.contains("Fourth"));
        assert!(notes.len() <= 240);
    }
}
