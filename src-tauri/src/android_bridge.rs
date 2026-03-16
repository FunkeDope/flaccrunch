/// Android bridge: query ContentResolver for content URI metadata.
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

// ---------------------------------------------------------------------------
// AndroidBridge — type-erased so callers don't need the Runtime generic
// ---------------------------------------------------------------------------

/// Holds a callable that wraps the Kotlin plugin handle.
/// Stored in Tauri app state on Android; never managed on desktop so
/// `try_state::<AndroidBridge>()` returns None there (safe fallback).
pub struct AndroidBridge(Box<dyn Fn(&str) -> Option<String> + Send + Sync>);

impl AndroidBridge {
    /// Query the real filename for an Android content URI via ContentResolver.
    /// Returns None if the query fails (Rust code falls back to URI-derived name).
    pub fn get_display_name(&self, uri: &str) -> Option<String> {
        (self.0)(uri)
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

                let bridge = AndroidBridge(Box::new(move |uri: &str| {
                    handle
                        .run_mobile_plugin::<GetDisplayNameResponse>(
                            "getDisplayName",
                            GetDisplayNamePayload { uri: uri.to_string() },
                        )
                        .ok()
                        .and_then(|r| r.display_name)
                        .filter(|n| !n.is_empty())
                }));

                app.manage(bridge);
            }

            // Suppress unused-variable warnings on desktop builds.
            #[cfg(not(target_os = "android"))]
            let _ = (app, api);

            Ok(())
        })
        .build()
}
