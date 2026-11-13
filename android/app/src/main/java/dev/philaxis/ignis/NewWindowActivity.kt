package dev.philaxis.ignis

import android.app.Activity
import android.os.Bundle

/**
 * Target of the "New window" app shortcut. Launcher shortcuts clear the task
 * they start in, so they cannot start the window directly without closing
 * the one that is already open; this invisible step opens a new one instead.
 */
class NewWindowActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        MainActivity.openNewWindow(this)
        finish()
    }
}
