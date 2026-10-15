//! Handing a file the program has just written to the machine it is running
//! on.
//!
//! Writing the file is the easy half. A reader then wants it somewhere else -
//! another computer, a chat, a drive - and what that takes is not the same
//! anywhere: a phone has a share sheet and no file manager a reader can reach,
//! a computer has a file manager and no share sheet worth using. A page in a
//! browser knows about neither and only has `<a download>`, which is right on
//! a computer and silently does nothing in the webview a phone shows.
//!
//! So the program says which of the two it can do, and the interface offers
//! that one.

use std::path::{Path, PathBuf};

#[cfg(target_os = "android")]
pub mod android;
#[cfg(target_os = "android")]
pub use android::note_machine;

#[cfg(target_os = "ios")]
mod apple;

/// Where this machine lets the program keep things it may throw away.
///
/// Only a phone answers. An app there has no home directory to work one out
/// from, so the framework hands the path to the shell as it starts and the
/// shell passes it on; a computer keeps nothing here and gets `None`.
pub fn cache_dir() -> Option<PathBuf> {
    std::env::var_os("TSUBURU_CACHE_DIR").map(PathBuf::from)
}

/// What can be done here with a file that has just been written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Handoff {
    /// Hand it to the operating system, which asks where it should go.
    /// A phone: there is no folder a reader can open instead.
    Share,
    /// Show it where it is, in the file manager. A computer: the reader
    /// already has somewhere to put things and a way to get at them.
    Reveal,
    /// Neither. The file is written and that is all that can be said.
    None,
}

/// Which of the two this machine does.
pub const fn handoff() -> Handoff {
    if cfg!(any(target_os = "android", target_os = "ios")) {
        Handoff::Share
    } else if cfg!(any(target_os = "macos", windows, unix)) {
        Handoff::Reveal
    } else {
        Handoff::None
    }
}

/// Hands the files over, however this machine does that.
///
/// Returns once the operating system has been asked. What the reader then
/// chooses - which app, which folder, or to dismiss it - is between them and
/// the operating system, and is not reported back: there is nothing the
/// program would do differently either way.
pub fn hand_over(paths: &[PathBuf]) -> Result<(), String> {
    let Some(first) = paths.first() else {
        return Err("nothing to hand over".into());
    };
    for path in paths {
        if !path.is_file() {
            return Err(format!("{} is not there to hand over", path.display()));
        }
    }
    match handoff() {
        Handoff::Share => share(paths),
        Handoff::Reveal => reveal(paths, first),
        Handoff::None => Err("this platform cannot be handed a file".into()),
    }
}

#[cfg(target_os = "android")]
fn share(paths: &[PathBuf]) -> Result<(), String> {
    android::share(paths)
}

#[cfg(target_os = "ios")]
fn share(paths: &[PathBuf]) -> Result<(), String> {
    apple::share(paths)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn share(_paths: &[PathBuf]) -> Result<(), String> {
    Err("this platform has no share sheet".into())
}

/// Opens the file manager on the files.
///
/// Selecting them rather than only opening the folder, where the platform's
/// file manager can be asked for that: a reader who exported eleven shards
/// wants to see which eleven they are, not a folder they then have to read.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn reveal(paths: &[PathBuf], first: &Path) -> Result<(), String> {
    // Detached: the file manager outlives this call, and a pipe left open to
    // it would keep a handle on the streams tsuburu's own logging wants.
    file_manager(paths, first)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not open the file manager: {e}"))
}

/// -R selects rather than opens, which for a `.tsd` would otherwise mean
/// handing it to whatever last claimed the extension.
#[cfg(target_os = "macos")]
fn file_manager(paths: &[PathBuf], _first: &Path) -> std::process::Command {
    let mut open = std::process::Command::new("open");
    open.arg("-R").args(paths);
    open
}

/// Explorer takes one file and no more.
#[cfg(windows)]
fn file_manager(_paths: &[PathBuf], first: &Path) -> std::process::Command {
    let mut explorer = std::process::Command::new("explorer");
    explorer.arg(format!("/select,{}", first.display()));
    explorer
}

/// No agreed way to ask for a file to be selected, so the folder it is in is
/// what can be opened.
#[cfg(all(unix, not(any(target_os = "macos", target_os = "android", target_os = "ios"))))]
fn file_manager(_paths: &[PathBuf], first: &Path) -> std::process::Command {
    let mut open = std::process::Command::new("xdg-open");
    open.arg(first.parent().unwrap_or(first));
    open
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn reveal(_paths: &[PathBuf], _first: &Path) -> Result<(), String> {
    Err("this platform has no file manager to open".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_computer_shows_a_file_and_a_phone_sends_it() {
        let expected = if cfg!(any(target_os = "android", target_os = "ios")) {
            Handoff::Share
        } else {
            Handoff::Reveal
        };
        assert_eq!(handoff(), expected);
    }

    #[test]
    fn nothing_to_hand_over_is_said_rather_than_ignored() {
        assert!(hand_over(&[]).is_err());
    }

    #[test]
    fn a_file_that_is_not_there_is_refused_before_the_platform_is_asked() {
        let missing = std::env::temp_dir().join("tsuburu-no-such-shard.tsd");
        let err = hand_over(&[missing]).unwrap_err();
        assert!(err.contains("not there"), "{err}");
    }
}
