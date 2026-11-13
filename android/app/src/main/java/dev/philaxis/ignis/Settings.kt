package dev.philaxis.ignis

import android.content.Context
import java.net.URI

/** Server address and vault, kept in the app's private preferences. */
class Settings(context: Context) {
    private val prefs = context.getSharedPreferences("settings", Context.MODE_PRIVATE)

    val server: URI?
        get() = prefs.getString(KEY_SERVER, null)?.let { runCatching { Urls.normalizeServer(it) }.getOrNull() }

    val vault: String?
        get() = prefs.getString(KEY_VAULT, null)?.takeIf { it.isNotEmpty() }

    fun save(server: URI, vault: String) {
        prefs.edit().putString(KEY_SERVER, server.toString()).putString(KEY_VAULT, vault.trim()).apply()
    }

    private companion object {
        const val KEY_SERVER = "server_url"
        const val KEY_VAULT = "vault"
    }
}
