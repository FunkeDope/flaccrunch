// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT
//
// FlacCrunch patch: changed ACTION_GET_CONTENT → ACTION_OPEN_DOCUMENT so that
// returned content URIs carry read + write grants.  Without this the
// android_write_back step in worker_pool.rs cannot open the URI for writing.
// takePersistableUriPermission is also called so the grants survive across
// the full processing session.

package app.tauri.dialog

import android.app.Activity
import android.app.AlertDialog
import android.content.Intent
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.webkit.MimeTypeMap
import androidx.activity.result.ActivityResult
import app.tauri.Logger
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class Filter {
  lateinit var extensions: Array<String>
}

@InvokeArg
class FilePickerOptions {
  lateinit var filters: Array<Filter>
  var multiple: Boolean? = null
  var pickerMode: String? = null
}

@InvokeArg
class MessageOptions {
  var title: String? = null
  lateinit var message: String
  var okButtonLabel: String? = null
  var noButtonLabel: String? = null
  var cancelButtonLabel: String? = null
}

@InvokeArg
class SaveFileDialogOptions {
  var fileName: String? = null
  lateinit var filters: Array<Filter>
}

@TauriPlugin
class DialogPlugin(private val activity: Activity): Plugin(activity) {
  var filePickerOptions: FilePickerOptions? = null

  @Command
  fun showFilePicker(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(FilePickerOptions::class.java)
      val parsedTypes = parseFiltersOption(args.filters)

      // ACTION_OPEN_DOCUMENT is the correct SAF flow for in-place editing.
      // Explicitly request read/write + persistable grants so the selected
      // document URI can be opened again later for overwrite.
      val intent = Intent(Intent.ACTION_OPEN_DOCUMENT)
      intent.addCategory(Intent.CATEGORY_OPENABLE)
      intent.addFlags(
        Intent.FLAG_GRANT_READ_URI_PERMISSION or
        Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
        Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
      )

      if (args.pickerMode == "image") {
        intent.type = "image/*"
      } else if (args.pickerMode == "video") {
        intent.type = "video/*"
      } else if (args.pickerMode == "media") {
        intent.type = "*/*"
        intent.putExtra(Intent.EXTRA_MIME_TYPES, arrayOf("video/*", "image/*"))
      } else if (parsedTypes.isNotEmpty()) {
        intent.type = "*/*"
        intent.putExtra(Intent.EXTRA_MIME_TYPES, parsedTypes)
      } else {
        intent.type = "*/*"
      }

      intent.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, args.multiple ?: false)

      startActivityForResult(invoke, intent, "filePickerResult")
    } catch (ex: Exception) {
      val message = ex.message ?: "Failed to pick file"
      Logger.error(message)
      invoke.reject(message)
    }
  }

  @ActivityCallback
  fun filePickerResult(invoke: Invoke, result: ActivityResult) {
    try {
      when (result.resultCode) {
        Activity.RESULT_OK -> {
          // Persist read + write URI permissions so write-back works for the
          // full processing session (not just the initial activity foreground).
          result.data?.let { intent ->
            val flags = intent.flags and (
              Intent.FLAG_GRANT_READ_URI_PERMISSION or
              Intent.FLAG_GRANT_WRITE_URI_PERMISSION
            )
            if (intent.clipData != null) {
              for (i in 0 until intent.clipData!!.itemCount) {
                try {
                  if (flags != 0) {
                    activity.contentResolver.takePersistableUriPermission(
                      intent.clipData!!.getItemAt(i).uri, flags
                    )
                  }
                } catch (e: Exception) { /* not all providers support persistable grants */ }
              }
            } else {
              intent.data?.let { uri ->
                try {
                  if (flags != 0) {
                    activity.contentResolver.takePersistableUriPermission(uri, flags)
                  }
                } catch (e: Exception) { /* ignore */ }
              }
            }
          }
          val callResult = createPickFilesResult(result.data)
          invoke.resolve(callResult)
        }
        Activity.RESULT_CANCELED -> invoke.reject("File picker cancelled")
        else -> invoke.reject("Failed to pick files")
      }
    } catch (ex: java.lang.Exception) {
      val message = ex.message ?: "Failed to read file pick result"
      Logger.error(message)
      invoke.reject(message)
    }
  }

  private fun createPickFilesResult(data: Intent?): JSObject {
    val callResult = JSObject()
    if (data == null) {
      callResult.put("files", null)
      return callResult
    }
    val uris: MutableList<String?> = ArrayList()
    if (data.clipData == null) {
      val uri: Uri? = data.data
      uris.add(uri?.toString())
    } else {
      for (i in 0 until data.clipData!!.itemCount) {
        val uri: Uri = data.clipData!!.getItemAt(i).uri
        uris.add(uri.toString())
      }
    }
    callResult.put("files", JSArray.from(uris.toTypedArray()))
    return callResult
  }

  private fun parseFiltersOption(filters: Array<Filter>): Array<String> {
    val mimeTypes = mutableListOf<String>()
    for (filter in filters) {
      for (ext in filter.extensions) {
        if (ext.contains('/')) {
          mimeTypes.add(if (ext == "text/csv") "text/comma-separated-values" else ext)
        } else {
          MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext)?.let {
            mimeTypes.add(it)
          }
        }
      }
    }
    return mimeTypes.toTypedArray()
  }

  @Command
  fun showMessageDialog(invoke: Invoke) {
    val args = invoke.parseArgs(MessageOptions::class.java)

    if (activity.isFinishing) {
      invoke.reject("App is finishing")
      return
    }

    val handler = { value: String ->
      val ret = JSObject()
      ret.put("value", value)
      invoke.resolve(ret)
    }

    Handler(Looper.getMainLooper())
      .post {
        val builder = AlertDialog.Builder(activity)

        if (args.title != null) {
          builder.setTitle(args.title)
        }

        val okButtonLabel = args.okButtonLabel ?: "Ok"

        builder
          .setMessage(args.message)
          .setPositiveButton(okButtonLabel) { dialog, _ ->
            dialog.dismiss()
            handler(okButtonLabel)
          }
          .setOnCancelListener { dialog ->
            dialog.dismiss()
            handler(args.cancelButtonLabel ?: "Cancel")
          }

        if (args.noButtonLabel != null) {
          builder.setNeutralButton(args.noButtonLabel) { dialog, _ ->
            dialog.dismiss()
            handler(args.noButtonLabel!!)
          }
        }

        if (args.cancelButtonLabel != null) {
          builder.setNegativeButton( args.cancelButtonLabel) { dialog, _ ->
            dialog.dismiss()
            handler(args.cancelButtonLabel!!)
          }
        }

        val dialog = builder.create()
        dialog.show()
      }
  }

  @Command
  fun saveFileDialog(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(SaveFileDialogOptions::class.java)
      val parsedTypes = parseFiltersOption(args.filters)

      val intent = Intent(Intent.ACTION_CREATE_DOCUMENT)
      intent.addCategory(Intent.CATEGORY_OPENABLE)
      intent.putExtra(Intent.EXTRA_TITLE, args.fileName ?: "")
      intent.type = "*/*"

      if (parsedTypes.isNotEmpty()) {
        intent.putExtra(Intent.EXTRA_MIME_TYPES, parsedTypes)
      }

      startActivityForResult(invoke, intent, "saveFileDialogResult")
    } catch (ex: Exception) {
      val message = ex.message ?: "Failed to pick save file"
      Logger.error(message)
      invoke.reject(message)
    }
  }

  @ActivityCallback
  fun saveFileDialogResult(invoke: Invoke, result: ActivityResult) {
    try {
      when (result.resultCode) {
        Activity.RESULT_OK -> {
          val callResult = JSObject()
          val intent: Intent? = result.data
          if (intent != null) {
            val uri = intent.data
            if (uri != null) {
              callResult.put("file", uri.toString())
            }
          }
          invoke.resolve(callResult)
        }
        Activity.RESULT_CANCELED -> invoke.reject("File picker cancelled")
        else -> invoke.reject("Failed to pick files")
      }
    } catch (ex: java.lang.Exception) {
      val message = ex.message ?: "Failed to read file pick result"
      Logger.error(message)
      invoke.reject(message)
    }
  }
}
