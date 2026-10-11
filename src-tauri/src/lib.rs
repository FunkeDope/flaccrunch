//! FlacCrunch library.
//!
//! The crate is split in two layers:
//!
//! * **Core** (always compiled): scanning, the per-file pipeline, libFLAC
//!   encoding/hashing/metadata, artwork optimization, run bookkeeping and
//!   logging. It has no dependency on Tauri and is what the headless
//!   `flaccrunch-cli` binary links against.
//! * **GUI** (`gui` feature, on by default): the Tauri application, IPC
//!   commands and the Android content-URI bridge.

pub mod artwork;
pub mod cli;
pub mod flac;
pub mod fs;
pub mod image;
pub mod logging;
pub mod pipeline;
pub mod state;
pub mod util;

#[cfg(feature = "gui")]
pub mod android_bridge;
#[cfg(feature = "gui")]
pub mod commands;
#[cfg(feature = "gui")]
pub mod gui;

/// Tauri entry point (mobile builds call this directly).
#[cfg(feature = "gui")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    gui::run_with_startup_paths(Vec::new());
}

#[cfg(feature = "gui")]
pub use gui::run_with_startup_paths;
