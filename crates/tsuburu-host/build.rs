//! Compiles the iOS share sheet alongside the Rust that calls it.
//!
//! Not left to Xcode. `tsuburu-mobile` is built as a `cdylib`, so cargo links
//! it itself and does so before Xcode compiles anything of its own: a symbol
//! only the Xcode project provided was undefined at that link, and the iOS
//! build failed. Compiled here it is in the library before anyone links it,
//! and it lives beside `apple.rs` rather than in a generated project that
//! `tauri ios init` rewrites.

fn main() {
    println!("cargo:rerun-if-changed=src/apple/share.mm");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("ios") {
        return;
    }
    cc::Build::new()
        .file("src/apple/share.mm")
        .cpp(true)
        // Objective-C++ rather than C++: the file is UIKit, and cc only knows
        // the difference from the extension on some versions.
        .flag("-x")
        .flag("objective-c++")
        // What the Xcode project compiles its own Objective-C with. Without
        // it the sheet and the URLs handed to it are never released.
        .flag("-fobjc-arc")
        .compile("tsuburu-share");
    println!("cargo:rustc-link-lib=framework=UIKit");
    println!("cargo:rustc-link-lib=framework=Foundation");
}
