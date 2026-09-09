//! The frontend is embedded from `web/dist`, which rust-embed requires to
//! exist while this crate compiles. It is not committed, and the frontend's
//! own build removes and recreates it, so create it here when it is absent:
//! a build that skips the frontend step then reaches the message that says
//! the assets are missing, instead of failing with a path error.

fn main() {
    let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/dist");
    if let Err(err) = std::fs::create_dir_all(&dist) {
        println!("cargo::warning=could not create {}: {err}", dist.display());
    }
    println!("cargo::rerun-if-changed=../../web/dist");
}
