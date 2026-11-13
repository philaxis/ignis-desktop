package dev.philaxis.ignis

import android.content.Intent
import android.os.Bundle
import android.text.InputType
import android.util.TypedValue
import android.view.Gravity
import android.view.ViewGroup
import android.view.inputmethod.EditorInfo
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.FrameLayout
import android.widget.ScrollView
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

/**
 * Asks for the server address and the vault, like the desktop app's first
 * window. Also opened by a link `dev.philaxis.ignis://settings?server=…&vault=…`,
 * which fills the fields in.
 */
class SettingsActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val settings = Settings(this)
        val link = intent.data

        val dp = { v: Int -> TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v.toFloat(), resources.displayMetrics).toInt() }
        val title = TextView(this).apply {
            text = getString(R.string.app_name)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 26f)
        }
        val lead = TextView(this).apply {
            text = getString(R.string.settings_lead)
            alpha = 0.7f
            setPadding(0, dp(4), 0, dp(20))
        }
        val server = EditText(this).apply {
            hint = "https://ignis.example.ts.net"
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI
            isSingleLine = true
            setText(link?.getQueryParameter("server") ?: settings.server?.toString().orEmpty())
        }
        val vault = EditText(this).apply {
            hint = "vault"
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
            isSingleLine = true
            imeOptions = EditorInfo.IME_ACTION_DONE
            setText(link?.getQueryParameter("vault") ?: settings.vault.orEmpty())
        }
        val error = TextView(this).apply {
            setTextColor(0xFFDD3333.toInt())
            setPadding(0, dp(12), 0, 0)
        }
        val save = Button(this).apply { text = getString(R.string.settings_save) }

        val label = { text: Int -> TextView(this).apply { setText(text); setPadding(0, dp(14), 0, dp(2)) } }
        val form = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(32), dp(28), dp(32), dp(28))
            addView(title)
            addView(lead)
            addView(label(R.string.settings_server))
            addView(server)
            addView(label(R.string.settings_vault))
            addView(vault)
            addView(TextView(this@SettingsActivity).apply { setText(R.string.settings_vault_hint); alpha = 0.65f })
            addView(save, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = dp(22) })
            addView(error)
        }
        // A column of readable width, centered on a tablet, full width in a
        // narrow split-screen pane.
        val maxWidth = dp(560)
        val scroll = object : ScrollView(this) {
            override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
                val width = minOf(MeasureSpec.getSize(widthMeasureSpec), maxWidth)
                super.onMeasure(MeasureSpec.makeMeasureSpec(width, MeasureSpec.EXACTLY), heightMeasureSpec)
            }
        }
        scroll.addView(form, ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
        val root = FrameLayout(this).apply {
            addView(scroll, FrameLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.MATCH_PARENT, Gravity.CENTER_HORIZONTAL))
        }
        ViewCompat.setOnApplyWindowInsetsListener(root) { view, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.ime())
            view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
            WindowInsetsCompat.CONSUMED
        }
        setContentView(root)

        save.setOnClickListener {
            val url = try {
                Urls.normalizeServer(server.text.toString())
            } catch (e: Urls.InvalidServer) {
                error.text = e.message
                return@setOnClickListener
            }
            settings.save(url, vault.text.toString())
            // Windows that are already open pick the change up when they come
            // back to the front; open one now if there is none.
            if (isTaskRoot) startActivity(Intent(this, MainActivity::class.java))
            finish()
        }
    }
}
