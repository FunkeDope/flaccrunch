package com.flaccrunch.app

import android.Manifest
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.ColorFilter
import android.graphics.Paint
import android.graphics.PixelFormat
import android.graphics.drawable.Drawable
import android.os.Build
import android.os.Bundle
import android.view.View
import android.view.WindowManager
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.annotation.RequiresApi
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import androidx.core.graphics.ColorUtils
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import org.json.JSONObject

class MainActivity : TauriActivity() {
    private val barsBackground = SystemBarsBackground()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setUpEdgeToEdge()
        requestAudioPermissionIfNeeded()
    }

    override fun onWebViewCreate(webView: WebView) {
        // Runs before the first page load, so the page sees the bridge on startup.
        webView.addJavascriptInterface(SystemUiBridge(), "FlacCrunchAndroid")
    }

    /**
     * Android 15 (targetSdk 35) always lays apps out edge-to-edge, so the
     * WebView would sit under the status bar, the camera cutout and the gesture
     * bar. Keep the WebView inside the safe area by padding its parent with the
     * system bar, cutout and keyboard insets, and paint the strips behind the
     * bars in colours the page reports (see src/lib/android.ts), so every theme
     * gets a status bar that matches its top edge.
     */
    private fun setUpEdgeToEdge() {
        WindowCompat.setDecorFitsSystemWindows(window, false)
        @Suppress("DEPRECATION")
        window.statusBarColor = Color.TRANSPARENT
        @Suppress("DEPRECATION")
        window.navigationBarColor = Color.TRANSPARENT
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            window.isStatusBarContrastEnforced = false
            window.isNavigationBarContrastEnforced = false
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            val attrs = window.attributes
            attrs.layoutInDisplayCutoutMode =
                WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
            window.attributes = attrs
        }

        val night = (resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) ==
            Configuration.UI_MODE_NIGHT_YES
        val initial = if (night) Color.rgb(0x14, 0x12, 0x18) else Color.rgb(0xFE, 0xF7, 0xFF)
        barsBackground.setColors(initial, initial)
        applyBarAppearance(initial, initial)

        val content = findViewById<View>(android.R.id.content)
        content.background = barsBackground
        ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
            val bars = insets.getInsets(
                WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
            )
            val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
            val bottom = maxOf(bars.bottom, ime.bottom)
            view.setPadding(bars.left, bars.top, bars.right, bottom)
            barsBackground.setInsets(bars.top, bottom)
            WindowInsetsCompat.CONSUMED
        }
        ViewCompat.requestApplyInsets(content)
    }

    /** Dark icons over light bars, light icons over dark bars. */
    private fun applyBarAppearance(top: Int, bottom: Int) {
        val controller = WindowCompat.getInsetsController(window, window.decorView)
        controller.isAppearanceLightStatusBars = ColorUtils.calculateLuminance(top) > 0.5
        controller.isAppearanceLightNavigationBars = ColorUtils.calculateLuminance(bottom) > 0.5
    }

    private fun requestAudioPermissionIfNeeded() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            // Android 13+: request READ_MEDIA_AUDIO for direct media file access
            if (ContextCompat.checkSelfPermission(this, Manifest.permission.READ_MEDIA_AUDIO)
                != PackageManager.PERMISSION_GRANTED
            ) {
                ActivityCompat.requestPermissions(
                    this,
                    arrayOf(Manifest.permission.READ_MEDIA_AUDIO),
                    1001
                )
            }
        }
        // Android 12 and below: READ_EXTERNAL_STORAGE is declared in the manifest
        // and will be requested by the system file picker flow automatically.
    }

    /** Exposed to the page as `window.FlacCrunchAndroid`. */
    inner class SystemUiBridge {
        /** Colours for the status bar strip and the navigation bar strip ("#rrggbb"). */
        @JavascriptInterface
        fun setSystemBarColors(top: String, bottom: String) {
            val topColor = parseColor(top) ?: return
            val bottomColor = parseColor(bottom) ?: topColor
            runOnUiThread {
                barsBackground.setColors(topColor, bottomColor)
                applyBarAppearance(topColor, bottomColor)
            }
        }

        /**
         * Material You (Android 12+): the wallpaper-derived system palettes as
         * JSON, `{"accent1": {"0": "#ffffff", "10": "#…", …}, …}`, or "" on
         * older Android versions.
         */
        @JavascriptInterface
        fun getDynamicColors(): String {
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) return ""
            return try {
                val json = JSONObject()
                for ((name, ids) in dynamicPalettes()) {
                    val tones = JSONObject()
                    TONES.forEachIndexed { index, tone ->
                        val color = ContextCompat.getColor(this@MainActivity, ids[index])
                        tones.put(tone.toString(), String.format("#%06x", color and 0xFFFFFF))
                    }
                    json.put(name, tones)
                }
                json.toString()
            } catch (e: Exception) {
                ""
            }
        }
    }

    private fun parseColor(value: String): Int? =
        try { Color.parseColor(value.trim()) } catch (e: IllegalArgumentException) { null }

    /** Fills the status bar strip with one colour and everything else with another. */
    private class SystemBarsBackground : Drawable() {
        private val paint = Paint()
        private var topColor = Color.BLACK
        private var bottomColor = Color.BLACK
        private var topInset = 0
        private var bottomInset = 0

        fun setColors(top: Int, bottom: Int) {
            topColor = top
            bottomColor = bottom
            invalidateSelf()
        }

        fun setInsets(top: Int, bottom: Int) {
            topInset = top
            bottomInset = bottom
            invalidateSelf()
        }

        override fun draw(canvas: Canvas) {
            val b = bounds
            paint.color = bottomColor
            canvas.drawRect(b, paint)
            paint.color = topColor
            canvas.drawRect(
                b.left.toFloat(), b.top.toFloat(), b.right.toFloat(),
                (b.bottom - bottomInset).coerceAtLeast(b.top + topInset).toFloat(), paint
            )
        }

        override fun setAlpha(alpha: Int) {}
        override fun setColorFilter(colorFilter: ColorFilter?) {}
        @Deprecated("Deprecated in Java")
        override fun getOpacity(): Int = PixelFormat.OPAQUE
    }

    private companion object {
        val TONES = intArrayOf(0, 10, 50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 1000)

        @RequiresApi(Build.VERSION_CODES.S)
        fun dynamicPalettes(): List<Pair<String, IntArray>> = listOf(
            "accent1" to intArrayOf(
                android.R.color.system_accent1_0, android.R.color.system_accent1_10,
                android.R.color.system_accent1_50, android.R.color.system_accent1_100,
                android.R.color.system_accent1_200, android.R.color.system_accent1_300,
                android.R.color.system_accent1_400, android.R.color.system_accent1_500,
                android.R.color.system_accent1_600, android.R.color.system_accent1_700,
                android.R.color.system_accent1_800, android.R.color.system_accent1_900,
                android.R.color.system_accent1_1000,
            ),
            "accent2" to intArrayOf(
                android.R.color.system_accent2_0, android.R.color.system_accent2_10,
                android.R.color.system_accent2_50, android.R.color.system_accent2_100,
                android.R.color.system_accent2_200, android.R.color.system_accent2_300,
                android.R.color.system_accent2_400, android.R.color.system_accent2_500,
                android.R.color.system_accent2_600, android.R.color.system_accent2_700,
                android.R.color.system_accent2_800, android.R.color.system_accent2_900,
                android.R.color.system_accent2_1000,
            ),
            "accent3" to intArrayOf(
                android.R.color.system_accent3_0, android.R.color.system_accent3_10,
                android.R.color.system_accent3_50, android.R.color.system_accent3_100,
                android.R.color.system_accent3_200, android.R.color.system_accent3_300,
                android.R.color.system_accent3_400, android.R.color.system_accent3_500,
                android.R.color.system_accent3_600, android.R.color.system_accent3_700,
                android.R.color.system_accent3_800, android.R.color.system_accent3_900,
                android.R.color.system_accent3_1000,
            ),
            "neutral1" to intArrayOf(
                android.R.color.system_neutral1_0, android.R.color.system_neutral1_10,
                android.R.color.system_neutral1_50, android.R.color.system_neutral1_100,
                android.R.color.system_neutral1_200, android.R.color.system_neutral1_300,
                android.R.color.system_neutral1_400, android.R.color.system_neutral1_500,
                android.R.color.system_neutral1_600, android.R.color.system_neutral1_700,
                android.R.color.system_neutral1_800, android.R.color.system_neutral1_900,
                android.R.color.system_neutral1_1000,
            ),
            "neutral2" to intArrayOf(
                android.R.color.system_neutral2_0, android.R.color.system_neutral2_10,
                android.R.color.system_neutral2_50, android.R.color.system_neutral2_100,
                android.R.color.system_neutral2_200, android.R.color.system_neutral2_300,
                android.R.color.system_neutral2_400, android.R.color.system_neutral2_500,
                android.R.color.system_neutral2_600, android.R.color.system_neutral2_700,
                android.R.color.system_neutral2_800, android.R.color.system_neutral2_900,
                android.R.color.system_neutral2_1000,
            ),
        )
    }
}
