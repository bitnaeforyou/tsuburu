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

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
pub use android::note_machine;

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
    #[error("could not open the installer: {0}")]
    Install(String),
    #[error("{0} is not what the release says it is")]
    NotAsPublished(String),
    #[error("this release was not signed by whoever publishes tsuburu")]
    NotSigned,
    #[error("the release points at {0}, which is not where it is published")]
    NotFromGitHub(String),
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

/// The file every release carries, naming what each of the others should
/// come to.
const SUMS: &str = "SHA256SUMS";

/// The signature over that file.
const SIGNATURE: &str = "SHA256SUMS.sig";

/// The key that says a release is ours.
///
/// Public, so it belongs in the source: this is the half that checks, and
/// the half that signs has never been on GitHub and never will be. It lives
/// on one machine beside the key that signs the Android package, which is
/// what makes this worth more than the digests alone - whoever takes the
/// account that publishes cannot sign with it.
///
/// Changing this abandons everyone running an older copy: their update will
/// refuse a release they cannot verify, which is the right answer and also a
/// dead end. Losing the other half means the same thing.
const PUBLISHER: [u8; 32] = [
    0x7e, 0xf5, 0x29, 0xbf, 0x94, 0x5a, 0xe8, 0x73, 0x23, 0x1c, 0x5d, 0xe6, 0x2a, 0xdc, 0x58, 0xc3,
    0xed, 0xc3, 0x8d, 0x2c, 0xd9, 0xe8, 0xf3, 0xc2, 0x46, 0xdd, 0xea, 0x86, 0x7a, 0x0d, 0x12, 0x54,
];

/// Hosts a release may be fetched from.
///
/// Each file's address comes out of the release listing rather than being
/// built here, so it is a value from the network saying where the next
/// version of this program should be downloaded from.
const PUBLISHERS: &[&str] =
    &["github.com", "objects.githubusercontent.com", "release-assets.githubusercontent.com"];

fn is_published_at(url: &str) -> bool {
    let host = url
        .strip_prefix("https://")
        .and_then(|rest| rest.split('/').next())
        .map(|host| host.split(':').next().unwrap_or(host))
        .unwrap_or_default();
    PUBLISHERS.contains(&host)
}

/// The digest for one file. `sha256sum` writes two spaces between the digest
/// and the name, and the name may contain one.
fn published_digest(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (digest, said) = line.split_once("  ")?;
        (said.trim() == name).then(|| digest.trim().to_ascii_lowercase())
    })
}

/// Whether the publisher signed this listing.
///
/// `verify_strict` rather than `verify`: it refuses the small-order keys and
/// signatures that make a plain check accept one thing as two.
fn signed_by_publisher(listing: &[u8], signature: &[u8]) -> Result<(), UpdateError> {
    use ed25519_dalek::{Signature, VerifyingKey};

    let key = VerifyingKey::from_bytes(&PUBLISHER).map_err(|_| UpdateError::NotSigned)?;
    let signature: [u8; 64] = signature.try_into().map_err(|_| UpdateError::NotSigned)?;
    key.verify_strict(listing, &Signature::from_bytes(&signature))
        .map_err(|_| UpdateError::NotSigned)
}

fn digest_of(path: &Path) -> Result<String, UpdateError> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).map_err(|e| UpdateError::Replace(e.to_string()))?;
    Ok(Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect())
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

/// Whether `name` is the phone package the release publishes.
///
/// Unlike every other file, its name carries the version, so there is
/// nothing constant to compare against.
pub fn is_package(name: &str) -> bool {
    name.starts_with("tsuburu-") && name.ends_with(".apk")
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
            match me.become_newer().await {
                Ok(version) => me.set(Progress::Ready { version }),
                Err(error) => {
                    tracing::warn!(%error, "the update could not be applied");
                    me.set(Progress::Failed { error: error.to_string() })
                }
            }
        });
    }

    /// Whatever this platform means by becoming the newer version.
    ///
    /// A computer writes the new program over the old one and is restarted; a
    /// phone cannot, and hands the package to the system installer instead.
    async fn become_newer(self: &Arc<Self>) -> Result<String, UpdateError> {
        #[cfg(target_os = "android")]
        return self.hand_to_installer().await;
        #[cfg(not(target_os = "android"))]
        return self.replace_self().await;
    }

    /// Fetches the phone package and opens the installer on it.
    ///
    /// It goes in the cache directory because that is the one place the
    /// `FileProvider` in the Android project is already told it may share
    /// from, and because a package that has been installed is rubbish.
    #[cfg(target_os = "android")]
    async fn hand_to_installer(self: &Arc<Self>) -> Result<String, UpdateError> {
        let release = self.latest().await?.ok_or(UpdateError::Unpublished)?;
        let asset = release
            .assets
            .iter()
            .find(|a| is_package(&a.name))
            .ok_or_else(|| UpdateError::NoAsset(release.tag_name.clone()))?
            .clone();

        let work = cache_dir()?.join("update");
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(&work).map_err(|e| UpdateError::Replace(e.to_string()))?;
        let apk = work.join(&asset.name);
        self.fetch_as_published(&release, &asset, &apk).await?;

        android::install(&apk).map_err(UpdateError::Install)?;
        Ok(release.tag_name.trim_start_matches('v').to_string())
    }

    #[cfg_attr(target_os = "android", allow(dead_code))]
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
        self.fetch_as_published(&release, &asset, &archive).await?;

        let status = tokio::process::Command::new(system_tar())
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

    /// Fetches one of a release's files, having checked it is the file the
    /// release says it is.
    ///
    /// What this does not do: the digests travel in the same release as the
    /// file, so it is no defence against whoever can publish one - only a key
    /// kept away from the publisher would be. What it does do is make a
    /// tampered transfer, a redirect onto another host and a truncated
    /// download all refuse rather than become the running program, and a
    /// release carrying no digests is refused rather than trusted.
    async fn fetch_as_published(
        self: &Arc<Self>,
        release: &Release,
        asset: &Asset,
        to: &Path,
    ) -> Result<(), UpdateError> {
        let sums = release
            .assets
            .iter()
            .find(|a| a.name == SUMS)
            .ok_or_else(|| UpdateError::NotAsPublished(SUMS.to_string()))?;
        for url in [&sums.browser_download_url, &asset.browser_download_url] {
            if !is_published_at(url) {
                return Err(UpdateError::NotFromGitHub(url.clone()));
            }
        }

        let signature =
            release.assets.iter().find(|a| a.name == SIGNATURE).ok_or(UpdateError::NotSigned)?;
        if !is_published_at(&signature.browser_download_url) {
            return Err(UpdateError::NotFromGitHub(signature.browser_download_url.clone()));
        }

        let listing = self.body(&sums.browser_download_url).await?;
        let signed = self.body(&signature.browser_download_url).await?;
        signed_by_publisher(&listing, &signed)?;

        let listing = String::from_utf8(listing).map_err(|_| UpdateError::NotSigned)?;
        let want = published_digest(&listing, &asset.name)
            .ok_or_else(|| UpdateError::NotAsPublished(asset.name.clone()))?;

        self.fetch(&asset.browser_download_url, to).await?;
        if digest_of(to)? != want {
            let _ = std::fs::remove_file(to);
            return Err(UpdateError::NotAsPublished(asset.name.clone()));
        }
        Ok(())
    }

    async fn body(self: &Arc<Self>, url: &str) -> Result<Vec<u8>, UpdateError> {
        Ok(self
            .client
            .get(url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| UpdateError::Unreachable(e.to_string()))?
            .bytes()
            .await
            .map_err(|e| UpdateError::Unreachable(e.to_string()))?
            .to_vec())
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

/// Where the phone lets this app keep things it may throw away.
///
/// Passed in rather than worked out: an Android app has no home directory to
/// derive one from, and only the framework knows the path.
#[cfg(target_os = "android")]
fn cache_dir() -> Result<PathBuf, UpdateError> {
    std::env::var_os("TSUBURU_CACHE_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| UpdateError::Replace("nowhere to keep the package".into()))
}

/// Where the system's own `tar` is.
///
/// Named in full rather than looked up: on Windows the search for a bare
/// command includes the directory the program is in, so an install directory
/// somebody else can write to is a way to be handed a different `tar` - and
/// this one is run on bytes that have just been downloaded.
fn system_tar() -> &'static str {
    if cfg!(windows) { r"C:\Windows\System32\tar.exe" } else { "/usr/bin/tar" }
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

    /// Made with the key that is kept off this machine's repository, by
    /// `openssl pkeyutl -sign -rawin`, which is what the release script runs.
    const SIGNED_LISTING: &[u8] = b"aaaa  tsuburu-x86_64-unknown-linux-gnu.tar.gz\n";
    const REAL_SIGNATURE: [u8; 64] = [
        0xdf, 0x82, 0x2a, 0x2a, 0x1b, 0xb5, 0x36, 0x68, 0x83, 0x26, 0xc4, 0x36, 0x2d, 0xa9, 0xfd,
        0xd3, 0x7e, 0x03, 0x07, 0x99, 0xdb, 0x55, 0xcf, 0xa2, 0x09, 0x88, 0xed, 0x02, 0x2a, 0x7f,
        0x5a, 0x6d, 0x19, 0xe8, 0x30, 0x1d, 0x53, 0x82, 0xef, 0x88, 0xbe, 0x60, 0x66, 0x69, 0xe3,
        0x65, 0x80, 0x19, 0x88, 0xe0, 0xad, 0x35, 0x48, 0x70, 0x56, 0x65, 0x8a, 0xb9, 0x7f, 0x3c,
        0x83, 0x20, 0xbb, 0x0f,
    ];

    #[test]
    fn a_listing_the_publisher_signed_is_accepted() {
        assert!(signed_by_publisher(SIGNED_LISTING, &REAL_SIGNATURE).is_ok());
    }

    #[test]
    fn a_listing_that_was_changed_after_signing_is_refused() {
        let tampered = b"bbbb  tsuburu-x86_64-unknown-linux-gnu.tar.gz\n";
        assert!(signed_by_publisher(tampered, &REAL_SIGNATURE).is_err());
    }

    #[test]
    fn a_signature_by_somebody_else_is_refused() {
        let mut theirs = REAL_SIGNATURE;
        theirs[0] ^= 0x01;
        assert!(signed_by_publisher(SIGNED_LISTING, &theirs).is_err());
        // And nothing at all where a signature should be.
        assert!(signed_by_publisher(SIGNED_LISTING, &[]).is_err());
        assert!(signed_by_publisher(SIGNED_LISTING, &[0u8; 63]).is_err());
    }

    #[test]
    fn only_github_is_fetched_from() {
        assert!(is_published_at("https://github.com/owner/repo/releases/download/v1/x.zip"));
        assert!(is_published_at("https://objects.githubusercontent.com/whatever"));
        assert!(!is_published_at("https://evil.example/x.zip"));
        assert!(!is_published_at("http://github.com/x.zip"));
        assert!(!is_published_at("https://github.com.evil.example/x.zip"));
    }

    #[test]
    fn a_digest_is_read_by_the_name_beside_it() {
        let sums = "\
aaaa  tsuburu-x86_64-pc-windows-msvc.zip
bbbb  tsuburu-aarch64-apple-darwin.zip
";
        assert_eq!(
            published_digest(sums, "tsuburu-aarch64-apple-darwin.zip").as_deref(),
            Some("bbbb")
        );
        assert_eq!(published_digest(sums, "tsuburu-0.4.0.apk"), None);
    }

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
