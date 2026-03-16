// FlacCrunch — Android bridge plugin for ContentResolver queries.
//
// Exposes getDisplayName: queries OpenableColumns.DISPLAY_NAME for a
// content URI, which is the standard way every Android file manager
// retrieves the human-readable filename for any URI returned by the
// Storage Access Framework (ACTION_OPEN_DOCUMENT / ACTION_GET_CONTENT).
//
// Placed in com.flaccrunch.bridge so it doesn't conflict with the
// auto-generated app package (com.flaccrunch.app).
//
// CI copies this file to:
//   src-tauri/gen/android/app/src/main/java/com/flaccrunch/bridge/
// after `npx tauri android init` so that the Tauri annotation processor
// includes it in the generated plugin registry.

package com.flaccrunch.bridge

import android.app.Activity
import android.net.Uri
import android.provider.OpenableColumns
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.FileInputStream

@TauriPlugin
class AndroidBridgePlugin(private val activity: Activity) : Plugin(activity) {

    /**
     * Query the real display name (filename) for a content URI using
     * Android's ContentResolver.  This is identical to what every Android
     * file manager, media player, and document editor does internally.
     *
     * Invoke args: { uri: String }
     * Response:    { displayName: String | null }
     */
    @Command
    fun getDisplayName(invoke: Invoke) {
        val uriStr = invoke.getString("uri") ?: run {
            invoke.reject("Missing uri parameter")
            return
        }

        var displayName: String? = null
        try {
            val uri = Uri.parse(uriStr)
            activity.contentResolver.query(
                uri,
                arrayOf(OpenableColumns.DISPLAY_NAME),
                null, null, null
            )?.use { cursor ->
                if (cursor.moveToFirst()) {
                    val idx = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                    if (idx >= 0) {
                        displayName = cursor.getString(idx)
                    }
                }
            }
        } catch (e: Exception) {
            // displayName stays null — Rust side will fall back to URI-derived name
        }

        val result = JSObject()
        result.put("displayName", displayName)
        invoke.resolve(result)
    }

    /**
     * Write the contents of a local cache file to a SAF content URI.
     *
     * Uses contentResolver.openOutputStream(uri, "wt") — the canonical Android
     * API for overwriting a file granted via ACTION_OPEN_DOCUMENT.  The "wt"
     * mode (write + truncate) is required; "w" alone leaves stale bytes when
     * the new content is shorter than the original on many providers.
     *
     * Invoke args: { cachePath: String, uri: String }
     * Response:    { ok: Boolean }
     */
    @Command
    fun writeCacheFileToUri(invoke: Invoke) {
        val cachePath = invoke.getString("cachePath") ?: run {
            invoke.reject("Missing cachePath parameter")
            return
        }
        val uriStr = invoke.getString("uri") ?: run {
            invoke.reject("Missing uri parameter")
            return
        }

        try {
            val uri = Uri.parse(uriStr)
            activity.contentResolver.openOutputStream(uri, "wt")?.use { outputStream ->
                FileInputStream(cachePath).use { inputStream ->
                    inputStream.copyTo(outputStream)
                }
            } ?: run {
                invoke.reject("contentResolver.openOutputStream returned null for URI: $uriStr")
                return
            }
        } catch (e: Exception) {
            invoke.reject("Write failed: ${e.message}")
            return
        }

        val result = JSObject()
        result.put("ok", true)
        invoke.resolve(result)
    }
}
