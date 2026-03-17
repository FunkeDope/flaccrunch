// FlacCrunch — Android bridge plugin for ContentResolver queries and SAF write.
//
// Uses Tauri's startActivityForResult + @ActivityCallback for file/folder pickers
// (the same mechanism DialogPlugin uses). This is the only supported way to handle
// activity results inside a Tauri plugin — registerForActivityResult from the
// Activity Result API fires too late and silently fails.
//
// Commands:
//   getDisplayName       — queries OpenableColumns.DISPLAY_NAME for a content URI
//   selectInputFiles     — opens ACTION_OPEN_DOCUMENT for SAF file picking
//   selectOutputFolder   — opens ACTION_OPEN_DOCUMENT_TREE for folder picking
//   writeFileToUri       — overwrites an existing SAF document URI
//   writeFileToFolder    — writes a cache file into a SAF tree URI folder

package com.flaccrunch.bridge

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.provider.OpenableColumns
import androidx.activity.result.ActivityResult
import androidx.documentfile.provider.DocumentFile
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
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

@InvokeArg
internal class WriteFileToUriArgs {
    lateinit var uri: String
    lateinit var cachePath: String
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

@TauriPlugin
class AndroidBridgePlugin(private val activity: Activity) : Plugin(activity) {

    private fun persistUriPermission(uri: Uri, flags: Int) {
        if (flags == 0) return
        try {
            activity.contentResolver.takePersistableUriPermission(uri, flags)
        } catch (_: Exception) {
            // Some providers do not support persistable grants. The transient
            // grant returned by the picker still covers the active session.
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
    // selectInputFiles — SAF file picker via Tauri's startActivityForResult
    // -----------------------------------------------------------------------

    @Command
    fun selectInputFiles(invoke: Invoke) {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
            )
            type = "*/*"
            putExtra(
                Intent.EXTRA_MIME_TYPES,
                arrayOf("audio/flac", "audio/x-flac", "application/x-flac")
            )
            putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true)
        }
        startActivityForResult(invoke, intent, "selectInputFilesResult")
    }

    @ActivityCallback
    fun selectInputFilesResult(invoke: Invoke, result: ActivityResult) {
        val jsResult = JSObject()
        val files = mutableListOf<String>()
        if (result.resultCode == Activity.RESULT_OK) {
            result.data?.let { intent ->
                val flags = intent.flags and (
                    Intent.FLAG_GRANT_READ_URI_PERMISSION or
                        Intent.FLAG_GRANT_WRITE_URI_PERMISSION
                    )
                if (intent.clipData != null) {
                    for (i in 0 until intent.clipData!!.itemCount) {
                        val uri = intent.clipData!!.getItemAt(i).uri
                        persistUriPermission(uri, flags)
                        files.add(uri.toString())
                    }
                } else {
                    intent.data?.let { uri ->
                        persistUriPermission(uri, flags)
                        files.add(uri.toString())
                    }
                }
            }
        }
        jsResult.put("files", JSArray.from(files.toTypedArray()))
        invoke.resolve(jsResult)
    }

    // -----------------------------------------------------------------------
    // selectOutputFolder — SAF folder picker via Tauri's startActivityForResult
    // -----------------------------------------------------------------------

    @Command
    fun selectOutputFolder(invoke: Invoke) {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
            )
        }
        startActivityForResult(invoke, intent, "selectOutputFolderResult")
    }

    @ActivityCallback
    fun selectOutputFolderResult(invoke: Invoke, result: ActivityResult) {
        val jsResult = JSObject()
        if (result.resultCode == Activity.RESULT_OK) {
            val treeUri = result.data?.data
            if (treeUri != null) {
                persistUriPermission(
                    treeUri,
                    Intent.FLAG_GRANT_READ_URI_PERMISSION or
                        Intent.FLAG_GRANT_WRITE_URI_PERMISSION
                )
                jsResult.put("treeUri", treeUri.toString())
            } else {
                jsResult.put("treeUri", null)
            }
        } else {
            jsResult.put("treeUri", null)
        }
        invoke.resolve(jsResult)
    }

    // -----------------------------------------------------------------------
    // writeFileToUri — overwrite an existing SAF document URI
    // -----------------------------------------------------------------------

    @Synchronized
    @Command
    fun writeFileToUri(invoke: Invoke) {
        val args = invoke.parseArgs(WriteFileToUriArgs::class.java)

        try {
            val uri = Uri.parse(args.uri)
            activity.contentResolver.openOutputStream(uri, "wt")?.use { out ->
                FileInputStream(args.cachePath).use { inp ->
                    inp.copyTo(out)
                }
            } ?: throw Exception("openOutputStream returned null for: ${args.uri}")

            val result = JSObject()
            result.put("ok", true)
            invoke.resolve(result)
        } catch (e: Exception) {
            invoke.reject("writeFileToUri failed: ${e.message}")
        }
    }

    // -----------------------------------------------------------------------
    // writeFileToFolder — write a cache file into a SAF tree URI folder
    // -----------------------------------------------------------------------

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
            val nameForCreate = if (args.filename.lowercase().endsWith(".flac")) {
                args.filename.dropLast(5)
            } else {
                args.filename
            }

            val newFile = treeDoc.createFile("audio/flac", nameForCreate)
                ?: throw Exception("createFile returned null for: ${args.filename}")

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
