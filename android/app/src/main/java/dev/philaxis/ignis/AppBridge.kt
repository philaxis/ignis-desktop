package dev.philaxis.ignis

import android.app.Activity
import android.content.Intent
import android.webkit.JavascriptInterface

/**
 * `window.IgnisApp` on the connect page: lets it open the server-address
 * screen while the server cannot be reached. It only starts that screen.
 */
class AppBridge(private val activity: Activity) {
    @JavascriptInterface
    fun openSettings() {
        activity.runOnUiThread {
            activity.startActivity(Intent(activity, SettingsActivity::class.java))
        }
    }

    companion object {
        const val NAME = "IgnisApp"
    }
}
