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
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.RadioButton
import android.widget.RadioGroup
import android.widget.ScrollView
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import java.net.HttpURLConnection
import java.net.URI
import java.net.URL
import org.json.JSONArray

/**
 * Asks for the server address and the vault. The vault is picked from the
 * list the server reports (`api/vault/list`): Ignis opens its first vault when
 * the URL names none, so the vault is never left to a guess. Also opened by a
 * link `dev.philaxis.ignis://settings?server=…&vault=…`, which fills both in.
 */
class SettingsActivity : ComponentActivity() {
    private lateinit var vaults: RadioGroup
    private lateinit var vaultStatus: TextView

    /** The vault to select once the list arrives: the saved one or the link's. */
    private var wantedVault: String? = null

    /** Server whose list is shown or loading; a newer request supersedes older ones. */
    private var listedServer: URI? = null
    private var listLoaded = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val settings = Settings(this)
        val link = intent.data
        wantedVault = link?.getQueryParameter("vault")?.takeIf { it.isNotEmpty() } ?: settings.vault

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
            hint = "e.g. https://ignis.example.ts.net"
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI
            isSingleLine = true
            imeOptions = EditorInfo.IME_ACTION_DONE
            setText(link?.getQueryParameter("server") ?: settings.server?.toString().orEmpty())
        }
        vaultStatus = TextView(this).apply { alpha = 0.65f }
        vaults = RadioGroup(this)
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
            addView(vaultStatus)
            addView(vaults)
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

        val loadFromField = {
            val url = runCatching { Urls.normalizeServer(server.text.toString()) }.getOrNull()
            if (url == null) showVaults(null, emptyList(), getString(R.string.settings_vault_need_server))
            else if (url != listedServer) loadVaults(url)
        }
        server.setOnEditorActionListener { _, _, _ -> loadFromField(); false }
        server.setOnFocusChangeListener { _, focused -> if (!focused) loadFromField() }
        loadFromField()

        save.setOnClickListener {
            val url = try {
                Urls.normalizeServer(server.text.toString())
            } catch (e: Urls.InvalidServer) {
                error.text = e.message
                return@setOnClickListener
            }
            if (url != listedServer) {
                loadVaults(url)
                error.text = getString(R.string.settings_vault_pick)
                return@setOnClickListener
            }
            val picked = vaults.findViewById<RadioButton>(vaults.checkedRadioButtonId)?.tag as String?
            // Without a list (server unreachable now) keep the vault chosen before.
            val vault = picked ?: wantedVault.takeIf { !listLoaded }
            if (vault == null) {
                error.text = getString(R.string.settings_vault_pick)
                return@setOnClickListener
            }
            settings.save(url, vault)
            // Windows that are already open pick the change up when they come
            // back to the front; open one now if there is none.
            if (isTaskRoot) startActivity(Intent(this, MainActivity::class.java))
            finish()
        }
    }

    private fun loadVaults(server: URI) {
        listedServer = server
        showVaults(server, emptyList(), getString(R.string.settings_vault_loading))
        Thread {
            val result = runCatching { fetchVaults(server) }
            runOnUiThread {
                if (isDestroyed || server != listedServer) return@runOnUiThread
                result.fold(
                    onSuccess = { list ->
                        showVaults(server, list, if (list.isEmpty()) getString(R.string.settings_vault_none) else null)
                    },
                    onFailure = { e ->
                        showVaults(server, emptyList(), getString(R.string.settings_vault_failed, e.message ?: e.javaClass.simpleName))
                    },
                )
            }
        }.start()
    }

    private fun showVaults(server: URI?, list: List<Vault>, status: String?) {
        listedServer = server
        listLoaded = server != null && status == null
        vaultStatus.text = status.orEmpty()
        vaultStatus.visibility = if (status == null) TextView.GONE else TextView.VISIBLE
        // Remember a choice made in a list that is about to be replaced.
        (vaults.findViewById<RadioButton>(vaults.checkedRadioButtonId)?.tag as String?)?.let { wantedVault = it }
        vaults.removeAllViews()
        for (vault in list) {
            val button = RadioButton(this).apply {
                id = android.view.View.generateViewId()
                text = vault.name
                minHeight = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, 48f, resources.displayMetrics).toInt()
                tag = vault.id
            }
            vaults.addView(button)
            if (vault.id == wantedVault) vaults.check(button.id)
        }
    }

    private class Vault(val id: String, val name: String)

    private fun fetchVaults(server: URI): List<Vault> {
        val connection = URL(Urls.origin(server) + server.rawPath + "api/vault/list").openConnection() as HttpURLConnection
        connection.connectTimeout = 8000
        connection.readTimeout = 8000
        try {
            if (connection.responseCode != 200) throw IllegalStateException("HTTP ${connection.responseCode}")
            val array = JSONArray(connection.inputStream.bufferedReader().use { it.readText() })
            return (0 until array.length()).map {
                val item = array.getJSONObject(it)
                val id = item.getString("id")
                Vault(id, item.optString("name").ifEmpty { id })
            }
        } finally {
            connection.disconnect()
        }
    }
}
