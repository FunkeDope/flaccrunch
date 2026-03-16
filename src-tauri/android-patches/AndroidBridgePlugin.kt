// FlacCrunch — Android bridge plugin for ContentResolver queries and SAF write.
//
// Commands:
//   getDisplayName       — queries OpenableColumns.DISPLAY_NAME for a content URI
//   selectOutputFolder   — opens ACTION_OPEN_DOCUMENT_TREE for the user to pick
//                          an output folder; returns a tree URI with full read+write
//   writeFileToFolder    — writes a local cache file into a SAF tree URI folder
//                          using DocumentFile (creates/overwrites by display name)

package com.flaccrunch.bridge

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.provider.OpenableColumns
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import androidx.documentfile.provider.DocumentFile
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.FileInputStream

// ---------------------------------------------------------------------------
// Arg types
// ---------------------------------------------------------------------------

@InvokeArg
internal class GetDisplayNameArgs {
    lateinit var uri: String
}

@InvokeArg
internal class WriteFileToFolderArgs {
    lateinit var treeUri: String
    lateinit var cachePath: String
    lateinit var filename: String
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

@TauriPlugin
class AndroidBridgePlugin(private val activity: Activity) : Plugin(activity) {

    // Stores the pending Invoke while the folder picker is on screen.
    private var pendingFolderInvoke: Invoke? = null

    // Registered in load() to satisfy the "before onStart" requirement of the
    // Activity Result API.  Nullable so devices/paths that never use it don't fail.
    private var folderPickerLauncher: ActivityResultLauncher<Intent>? = null

    /**
     * Called by the Tauri plugin framework during Activity.onCreate — before onStart.
     * This is the correct place to call registerForActivityResult.
     */
    override fun load(webView: android.webkit.WebView) {
        super.load(webView)
        try {
            folderPickerLauncher = (activity as ComponentActivity)
                .registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
                    val invoke = pendingFolderInvoke ?: return@registerForActivityResult
                    pendingFolderInvoke = null

                    val jsResult = JSObject()
                    if (result.resultCode == Activity.RESULT_OK) {
                        val treeUri = result.data?.data
                        if (treeUri != null) {
                            // Persist read+write grants so the URI survives across sessions.
                            try {
                                val flags = Intent.FLAG_GRANT_READ_URI_PERMISSION or
                                            Intent.FLAG_GRANT_WRITE_URI_PERMISSION
                                activity.contentResolver.takePersistableUriPermission(treeUri, flags)
                            } catch (_: Exception) { /* not all providers support persistable grants */ }
                            jsResult.put("treeUri", treeUri.toString())
                        } else {
                            jsResult.put("treeUri", null)
                        }
                    } else {
                        jsResult.put("treeUri", null)
                    }
                    invoke.resolve(jsResult)
                }
        } catch (_: Exception) {
            // If registration fails (e.g. wrong Activity type), selectOutputFolder
            // will return treeUri=null and Rust will abort the run.
        }
    }

    // -----------------------------------------------------------------------
    // getDisplayName — resolve the real filename from a content URI
    // -----------------------------------------------------------------------

    @Command
    fun getDisplayName(invoke: Invoke) {
        val args = invoke.parseArgs(GetDisplayNameArgs::class.java)
        var displayName: String? = null
        try {
            val uri = Uri.parse(args.uri)
            activity.contentResolver.query(
                uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null
            )?.use { cursor ->
                if (cursor.moveToFirst()) {
                    val idx = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                    if (idx >= 0) displayName = cursor.getString(idx)
                }
            }
        } catch (_: Exception) { }
        val result = JSObject()
        result.put("displayName", displayName)
        invoke.resolve(result)
    }

    // -----------------------------------------------------------------------
    // selectOutputFolder — open the SAF folder picker
    // -----------------------------------------------------------------------

    /**
     * Open ACTION_OPEN_DOCUMENT_TREE so the user can pick an output folder.
     * Returns { treeUri: String? } — null if the user cancels or the launcher
     * wasn't registered.
     */
    @Command
    fun selectOutputFolder(invoke: Invoke) {
        val launcher = folderPickerLauncher
        if (launcher == null) {
            val result = JSObject()
            result.put("treeUri", null)
            invoke.resolve(result)
            return
        }

        pendingFolderInvoke = invoke
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE)
        launcher.launch(intent)
    }

    // -----------------------------------------------------------------------
    // writeFileToFolder — write a cache file into a SAF tree URI folder
    // -----------------------------------------------------------------------

    /**
     * Write the contents of a local cache file into a folder selected via
     * ACTION_OPEN_DOCUMENT_TREE.
     *
     * Uses DocumentFile to create (or overwrite) the file by display name.
     * Synchronized to prevent race conditions when multiple workers write
     * to the same folder concurrently.
     *
     * Invoke args: { treeUri: String, cachePath: String, filename: String }
     * Response:    { ok: Boolean }
     */
    @Synchronized
    @Command
    fun writeFileToFolder(invoke: Invoke) {
        val args = invoke.parseArgs(WriteFileToFolderArgs::class.java)

        try {
            val treeUri = Uri.parse(args.treeUri)
            val treeDoc = DocumentFile.fromTreeUri(activity, treeUri)
                ?: throw Exception("Cannot open tree URI: ${args.treeUri}")

            // Remove existing file with the same name to avoid "track (1).flac" duplicates.
            treeDoc.findFile(args.filename)?.delete()

            // Strip the .flac extension for createFile — Android adds it back from the MIME type.
            // If the MIME type isn't recognized, pass the full filename to be safe.
            val nameForCreate = if (args.filename.lowercase().endsWith(".flac")) {
                args.filename.dropLast(5)  // remove ".flac"
            } else {
                args.filename
            }

            val newFile = treeDoc.createFile("audio/flac", nameForCreate)
                ?: throw Exception("createFile returned null for: ${args.filename}")

            // Verify the created file has the expected name. If Android didn't recognize
            // the MIME type, the extension might be wrong or missing.
            activity.contentResolver.openOutputStream(newFile.uri, "wt")?.use { out ->
                FileInputStream(args.cachePath).use { inp ->
                    inp.copyTo(out)
                }
            } ?: throw Exception("openOutputStream returned null for: ${newFile.uri}")

            val result = JSObject()
            result.put("ok", true)
            invoke.resolve(result)
        } catch (e: Exception) {
            invoke.reject("writeFileToFolder failed: ${e.message}")
        }
    }
}
