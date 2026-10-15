//! Asking iOS to show the share sheet.
//!
//! Unlike Android there is no way to reach UIKit from Rust, so the call goes
//! the other way: the iOS project carries a small Objective-C++ file that
//! presents the sheet, and this names it. The symbol is resolved when the
//! app is linked, which is the only build that reaches this module.

use std::ffi::{CString, c_char, c_int};
use std::path::PathBuf;

unsafe extern "C" {
    /// Defined in `src/apple/share.mm`, compiled into this library by its
    /// own build script rather than by Xcode. Returns zero
    /// once the sheet has been asked for, and a negative number if there was
    /// no window to present it over.
    fn tsuburu_share_files(paths: *const *const c_char, count: c_int) -> c_int;
}

pub fn share(paths: &[PathBuf]) -> Result<(), String> {
    // Held in a vector of their own: the array handed over points into these,
    // so they have to outlive the call rather than the expression that made
    // them.
    let held = paths
        .iter()
        .map(|path| CString::new(path.as_os_str().as_encoded_bytes()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "a file name with a zero byte in it cannot be handed over".to_string())?;
    let pointers: Vec<*const c_char> = held.iter().map(|c| c.as_ptr()).collect();

    let count = c_int::try_from(pointers.len()).map_err(|_| "too many files".to_string())?;
    // SAFETY: the array and everything it points at are alive for the call,
    // and `count` is its length.
    match unsafe { tsuburu_share_files(pointers.as_ptr(), count) } {
        0 => Ok(()),
        _ => Err("there was no window to show the share sheet over".into()),
    }
}
