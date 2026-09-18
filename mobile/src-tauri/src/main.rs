// A phone starts the library through the platform's own entry point; this is
// only here for running the same thing on a desktop.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tsuburu_mobile_lib::run()
}
