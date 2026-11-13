package dev.philaxis.ignis

import java.net.URI
import java.net.URISyntaxException
import java.net.URLDecoder
import java.net.URLEncoder

/**
 * Pure URL logic, the Android counterpart of `src-tauri/src/urls.rs`:
 * normalising the server address, building window URLs and deciding what a
 * `window.open` request from the page turns into.
 */
object Urls {
    /** Origin of the app's own pages (the connect page), served from assets. */
    const val APP_ASSETS = "https://appassets.androidplatform.net/assets/"

    class InvalidServer(message: String) : Exception(message)

    /**
     * Turns whatever the user typed into a canonical server URL: http(s)
     * only, no query, fragment or credentials, path ending in `/`. A missing
     * scheme defaults to `https://`.
     */
    fun normalizeServer(input: String): URI {
        val trimmed = input.trim()
        if (trimmed.isEmpty()) throw InvalidServer("Enter the address of your Ignis server.")
        val withScheme = if ("://" in trimmed) trimmed else "https://$trimmed"
        val parsed = try {
            URI(withScheme)
        } catch (e: URISyntaxException) {
            throw InvalidServer("Not a valid address: ${e.reason}")
        }
        val scheme = parsed.scheme?.lowercase()
        if (scheme != "http" && scheme != "https") {
            throw InvalidServer("The address must start with http:// or https://.")
        }
        val host = parsed.host ?: throw InvalidServer("The address has no host name.")
        val path = (parsed.rawPath ?: "").let { if (it.endsWith("/")) it else "$it/" }
        val port = if (parsed.port == -1) "" else ":${parsed.port}"
        return URI("$scheme://${host.lowercase()}$port$path")
    }

    /** `scheme://host[:port]`, the form WebView origin rules expect. */
    fun origin(server: URI): String =
        if (server.port == -1) "${server.scheme}://${server.host}" else "${server.scheme}://${server.host}:${server.port}"

    /** Same escaping as JavaScript's `encodeURIComponent`. */
    fun encodeComponent(value: String): String =
        URLEncoder.encode(value, "UTF-8")
            .replace("+", "%20")
            .replace("%21", "!")
            .replace("%27", "'")
            .replace("%28", "(")
            .replace("%29", ")")
            .replace("%7E", "~")

    /** The URL a window loads: `<server>?vault=…&workspace=…&file=…`. */
    fun windowUrl(server: URI, vault: String?, workspace: String?, file: String?): String {
        val query = listOf("vault" to vault, "workspace" to workspace, "file" to file)
            .filter { !it.second.isNullOrEmpty() }
            .joinToString("&") { (key, value) -> "$key=${encodeComponent(value!!)}" }
        val base = origin(server) + server.rawPath
        return if (query.isEmpty()) base else "$base?$query"
    }

    /** The local page that waits for the server to answer, then opens `target`. */
    fun launcherUrl(target: String): String = "${APP_ASSETS}connect.html?target=${encodeComponent(target)}"

    sealed interface NewWindowTarget {
        /** Another app window on the same server. */
        data class Window(val vault: String?, val file: String?) : NewWindowTarget
        /** Hand the link to another app (the browser, mail). */
        data class External(val url: String) : NewWindowTarget
        /** Do nothing: pop-outs (Obsidian has none on tablets), blob:, data:, … */
        data object Ignore : NewWindowTarget
    }

    /**
     * Classifies a `window.open` / `target=_blank` request, as the desktop app
     * does, except that `about:blank` pop-outs are ignored.
     */
    fun classifyNewWindow(server: URI, requested: String): NewWindowTarget {
        val url = try {
            URI(requested)
        } catch (e: URISyntaxException) {
            return NewWindowTarget.Ignore
        }
        return when (url.scheme?.lowercase()) {
            "http", "https" ->
                if (sameOrigin(server, url) && (url.rawPath.ifEmpty { "/" }) == server.rawPath) {
                    NewWindowTarget.Window(queryValue(url, "vault"), queryValue(url, "file"))
                } else {
                    NewWindowTarget.External(requested)
                }
            "mailto" -> NewWindowTarget.External(requested)
            else -> NewWindowTarget.Ignore
        }
    }

    fun sameOrigin(a: URI, b: URI): Boolean =
        a.scheme.equals(b.scheme, ignoreCase = true) &&
            a.host.equals(b.host, ignoreCase = true) &&
            effectivePort(a) == effectivePort(b)

    private fun effectivePort(u: URI): Int = when {
        u.port != -1 -> u.port
        u.scheme.equals("https", ignoreCase = true) -> 443
        else -> 80
    }

    private fun queryValue(url: URI, key: String): String? =
        url.rawQuery?.split("&")
            ?.map { it.split("=", limit = 2) }
            ?.firstOrNull { URLDecoder.decode(it[0], "UTF-8") == key }
            ?.getOrNull(1)
            ?.let { URLDecoder.decode(it, "UTF-8") }
            ?.takeIf { it.isNotEmpty() }
}
