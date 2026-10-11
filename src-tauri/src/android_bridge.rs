//! Android bridge: query ContentResolver for content URI metadata, open the
//! SAF document picker, and overwrite SAF document URIs with local files.
//!
//! Follows the same PluginHandle pattern as tauri-plugin-fs/src/mobile.rs.
//! On non-Android builds this module compiles but the plugin is a no-op.

use serde::{Deserialize, Serialize};
use tauri::{plugin::Builder, Runtime};

// ---------------------------------------------------------------------------
// Wire types (Rust ↔ Kotlin)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
struct GetDisplayNamePayload {
    uri: String,
}

#[derive(Deserialize)]
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
struct GetDisplayNameResponse {
    #[serde(rename = "displayName")]
    display_name: Option<String>,
}

#[derive(Serialize)]
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
struct SelectInputFilesPayload {}

#[derive(Deserialize, Default)]
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
struct SelectInputFilesResponse {
    #[serde(default)]
    files: Vec<String>,
}

#[derive(Serialize)]
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
struct WriteFileToUriPayload {
    uri: String,
    #[serde(rename = "cachePath")]
    cache_path: String,
}

#[derive(Deserialize)]
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
struct WriteFileToUriResponse {
    ok: bool,
}

// ---------------------------------------------------------------------------
// AndroidBridge — type-erased so callers don't need the Runtime generic
// ---------------------------------------------------------------------------

type DisplayNameFn = dyn Fn(&str) -> Option<String> + Send + Sync;
type SelectFilesFn = dyn Fn() -> Vec<String> + Send + Sync;
type WriteToUriFn = dyn Fn(&str, &str) -> Result<(), String> + Send + Sync;

/// Holds callables that wrap the Kotlin plugin handle.
/// Stored in Tauri app state on Android; never managed on desktop so
/// `try_state::<AndroidBridge>()` returns None there (safe fallback).
pub struct AndroidBridge {
    get_display_name: Box<DisplayNameFn>,
    /// Open the SAF file picker (ACTION_OPEN_DOCUMENT) and return the selected
    /// document URIs. Used on Android instead of tauri-plugin-dialog so we can
    /// control writable grant flags for in-place overwrite.
    select_input_files: Box<SelectFilesFn>,
    /// Overwrite an existing SAF document URI with the contents of a local
    /// cache file via Android's ContentResolver.
    write_file_to_uri: Box<WriteToUriFn>,
}

impl AndroidBridge {
    /// Query the real filename for an Android content URI via ContentResolver.
    /// Returns None if the query fails (Rust code falls back to URI-derived name).
    pub fn get_display_name(&self, uri: &str) -> Option<String> {
        (self.get_display_name)(uri)
    }

    /// Open the system document picker and return the selected file URIs.
    pub fn select_input_files(&self) -> Vec<String> {
        (self.select_input_files)()
    }

    /// Overwrite an existing content URI with the bytes from `cache_path`.
    pub fn write_file_to_uri(&self, uri: &str, cache_path: &str) -> Result<(), String> {
        (self.write_file_to_uri)(uri, cache_path)
    }
}

// ---------------------------------------------------------------------------
// Tauri plugin — registers AndroidBridgePlugin.kt on Android
// ---------------------------------------------------------------------------

pub fn plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    Builder::<R>::new("android-bridge")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            {
                use tauri::Manager;

                let handle =
                    api.register_android_plugin("com.flaccrunch.bridge", "AndroidBridgePlugin")?;
                let handle2 = handle.clone();
                let handle3 = handle.clone();

                let bridge = AndroidBridge {
                    get_display_name: Box::new(move |uri: &str| {
                        handle
                            .run_mobile_plugin::<GetDisplayNameResponse>(
                                "getDisplayName",
                                GetDisplayNamePayload {
                                    uri: uri.to_string(),
                                },
                            )
                            .ok()
                            .and_then(|r| r.display_name)
                            .filter(|n| !n.is_empty())
                    }),
                    select_input_files: Box::new(move || {
                        handle2
                            .run_mobile_plugin::<SelectInputFilesResponse>(
                                "selectInputFiles",
                                SelectInputFilesPayload {},
                            )
                            .map(|r| r.files)
                            .unwrap_or_default()
                    }),
                    write_file_to_uri: Box::new(move |uri: &str, cache_path: &str| {
                        handle3
                            .run_mobile_plugin::<WriteFileToUriResponse>(
                                "writeFileToUri",
                                WriteFileToUriPayload {
                                    uri: uri.to_string(),
                                    cache_path: cache_path.to_string(),
                                },
                            )
                            .map_err(|e| format!("writeFileToUri plugin call failed: {e}"))
                            .and_then(|r| {
                                if r.ok {
                                    Ok(())
                                } else {
                                    Err("writeFileToUri returned ok=false".to_string())
                                }
                            })
                    }),
                };

                app.manage(bridge);
            }

            // Suppress unused-variable warnings on desktop builds.
            #[cfg(not(target_os = "android"))]
            let _ = (app, api);

            Ok(())
        })
        .build()
}
