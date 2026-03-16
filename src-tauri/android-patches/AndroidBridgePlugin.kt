// FlacCrunch — Android bridge plugin for ContentResolver queries and SAF write-back.
//
// Commands:
//   getDisplayName  — queries OpenableColumns.DISPLAY_NAME for a content URI,
//                     the standard Android way to resolve the real filename.
//   writeCacheFileToUri — writes a local cache file back to a SAF content URI
//                         via contentResolver.openOutputStream(uri, "wt").
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
import android.provider.MediaStore
import android.provider.OpenableColumns
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.FileInputStream
import java.io.FileOutputStream

// ---------------------------------------------------------------------------
// Arg types — @InvokeArg with lateinit var gives non-null String after parseArgs,
// avoiding the String? overload-resolution ambiguity in constructors like
// FileInputStream(String!/File!/FileDescriptor!).
// This is the same pattern used in the project's DialogPlugin.kt.
// ---------------------------------------------------------------------------

@InvokeArg
internal class GetDisplayNameArgs {
    lateinit var uri: String
}

@InvokeArg
internal class WriteCacheFileArgs {
    lateinit var cachePath: String
    lateinit var uri: String
}

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
        val args = invoke.parseArgs(GetDisplayNameArgs::class.java)

        var displayName: String? = null
        try {
            val uri = Uri.parse(args.uri)
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
     * Primary path: contentResolver.openOutputStream(uri, "wt") — the canonical
     * Android API for overwriting a file granted via ACTION_OPEN_DOCUMENT.
     * The "wt" mode (write + truncate) is required; "w" alone leaves stale bytes
     * when the new content is shorter than the original on many providers.
     *
     * Fallback path (Android ≤ 10 with requestLegacyExternalStorage="true"):
     * When the primary write fails (e.g. DownloadStorageProvider denies writes to
     * msf: URIs even with ACTION_OPEN_DOCUMENT grants), we resolve the real file
     * path via MediaColumns.DATA and write directly.  On Android 11+ with scoped
     * storage this fallback returns false and the Rust layer uses fc-output instead.
     *
     * Invoke args: { cachePath: String, uri: String }
     * Response:    { ok: Boolean }
     */
    @Command
    fun writeCacheFileToUri(invoke: Invoke) {
        val args = invoke.parseArgs(WriteCacheFileArgs::class.java)
        // args.cachePath and args.uri are non-null String (guaranteed by @InvokeArg
        // lateinit var), so FileInputStream(args.cachePath) resolves unambiguously
        // to FileInputStream(String!) without any nullable overload confusion.

        val uri = Uri.parse(args.uri)
        try {
            activity.contentResolver.openOutputStream(uri, "wt")?.use { outputStream ->
                FileInputStream(args.cachePath).use { inputStream ->
                    inputStream.copyTo(outputStream)
                }
            } ?: run {
                // openOutputStream returned null — try the DATA path fallback.
                if (!tryWriteViaRealPath(args.cachePath, uri)) {
                    invoke.reject("contentResolver.openOutputStream returned null for URI: ${args.uri}")
                    return
                }
            }
        } catch (e: Exception) {
            // Primary write failed (e.g. DownloadStorageProvider requires MANAGE_DOCUMENTS
            // for msf: URIs even when ACTION_OPEN_DOCUMENT grants are in place).
            // Fall back to MediaColumns.DATA direct write (works on Android ≤ 10).
            if (!tryWriteViaRealPath(args.cachePath, uri)) {
                invoke.reject("Write failed: ${e.message}")
                return
            }
        }

        val result = JSObject()
        result.put("ok", true)
        invoke.resolve(result)
    }

    /**
     * Fallback write path: resolve the real filesystem path from the content URI via
     * MediaStore.MediaColumns.DATA and write directly with FileOutputStream.
     *
     * Works on Android ≤ 10 (legacy external storage) and some Android 10 devices
     * with requestLegacyExternalStorage="true".  On Android 11+ with scoped storage
     * enforcement, DATA is typically null or the path is not writable without
     * MANAGE_EXTERNAL_STORAGE, so this returns false gracefully.
     */
    private fun tryWriteViaRealPath(cachePath: String, contentUri: Uri): Boolean {
        return try {
            val projection = arrayOf(MediaStore.MediaColumns.DATA)
            val realPath = activity.contentResolver.query(
                contentUri, projection, null, null, null
            )?.use { cursor ->
                if (cursor.moveToFirst()) {
                    val col = cursor.getColumnIndex(MediaStore.MediaColumns.DATA)
                    if (col >= 0) cursor.getString(col) else null
                } else null
            }
            if (realPath.isNullOrEmpty()) return false
            FileOutputStream(realPath).use { out ->
                FileInputStream(cachePath).use { it.copyTo(out) }
            }
            true
        } catch (_: Exception) {
            false
        }
    }
}
