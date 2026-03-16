/// Android bridge: query ContentResolver for content URI metadata, and write
/// cache files back to SAF-granted content URIs.
///
/// Follows the exact same PluginHandle pattern as tauri-plugin-fs/src/mobile.rs.
/// On non-Android builds this module compiles but the plugin is a no-op.
use serde::{Deserialize, Serialize};
use tauri::{Runtime, plugin::Builder};

// ---------------------------------------------------------------------------
// Wire types (Rust ↔ Kotlin)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct GetDisplayNamePayload {
    uri: String,
}

#[derive(Deserialize)]
struct GetDisplayNameResponse {
    #[serde(rename = "displayName")]
    display_name: Option<String>,
}

#[derive(Serialize)]
struct WriteCacheFileToUriPayload {
    #[serde(rename = "cachePath")]
    cache_path: String,
    uri: String,
}

#[derive(Deserialize)]
struct WriteCacheFileToUriResponse {
    ok: bool,
}

// ---------------------------------------------------------------------------
// AndroidBridge — type-erased so callers don't need the Runtime generic
// ---------------------------------------------------------------------------

/// Holds callables that wrap the Kotlin plugin handle.
/// Stored in Tauri app state on Android; never managed on desktop so
/// `try_state::<AndroidBridge>()` returns None there (safe fallback).
pub struct AndroidBridge {
    get_display_name: Box<dyn Fn(&str) -> Option<String> + Send + Sync>,
    write_cache_to_uri: Box<dyn Fn(&str, &str) -> Result<(), String> + Send + Sync>,
}

impl AndroidBridge {
    /// Query the real filename for an Android content URI via ContentResolver.
    /// Returns None if the query fails (Rust code falls back to URI-derived name).
    pub fn get_display_name(&self, uri: &str) -> Option<String> {
        (self.get_display_name)(uri)
    }

    /// Write the contents of `cache_path` (a local filesystem path) to the
    /// SAF content URI `uri` via `contentResolver.openOutputStream(uri, "wt")`.
    /// This is the canonical Android write-back for ACTION_OPEN_DOCUMENT grants.
    pub fn write_cache_to_uri(&self, cache_path: &str, uri: &str) -> Result<(), String> {
        (self.write_cache_to_uri)(cache_path, uri)
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
                let handle = api.register_android_plugin(
                    "com.flaccrunch.bridge",
                    "AndroidBridgePlugin",
                )?;

                // Clone the handle so it can be moved into two separate closures.
                // tauri 2.x PluginHandle implements Clone.
                let handle2 = handle.clone();

                let bridge = AndroidBridge {
                    get_display_name: Box::new(move |uri: &str| {
                        handle
                            .run_mobile_plugin::<GetDisplayNameResponse>(
                                "getDisplayName",
                                GetDisplayNamePayload { uri: uri.to_string() },
                            )
                            .ok()
                            .and_then(|r| r.display_name)
                            .filter(|n| !n.is_empty())
                    }),
                    write_cache_to_uri: Box::new(move |cache_path: &str, uri: &str| {
                        handle2
                            .run_mobile_plugin::<WriteCacheFileToUriResponse>(
                                "writeCacheFileToUri",
                                WriteCacheFileToUriPayload {
                                    cache_path: cache_path.to_string(),
                                    uri: uri.to_string(),
                                },
                            )
                            .map_err(|e| format!("writeCacheFileToUri plugin call failed: {e}"))
                            .and_then(|r| {
                                if r.ok {
                                    Ok(())
                                } else {
                                    Err("writeCacheFileToUri returned ok=false".to_string())
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
