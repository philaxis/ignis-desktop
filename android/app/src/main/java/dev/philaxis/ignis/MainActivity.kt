package dev.philaxis.ignis

import android.Manifest
import android.annotation.SuppressLint
import android.app.ActivityManager
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Bundle
import android.os.Message
import android.view.ViewGroup
import android.webkit.PermissionRequest
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import androidx.activity.ComponentActivity
import androidx.activity.addCallback
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.webkit.ScriptHandler
import androidx.webkit.WebViewAssetLoader
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import java.net.URI

/**
 * One app window: a full-screen WebView on the Ignis server, in Obsidian's
 * tablet layout. Several can run at once, each in its own task (side by side
 * in split screen), and each has its own Ignis workspace.
 */
class MainActivity : ComponentActivity() {
    private lateinit var web: WebView
    private lateinit var windowId: String
    private var loaded: Pair<URI, String?>? = null
    private var tabletScript: ScriptHandler? = null
    private var fileCallback: ValueCallback<Array<Uri>>? = null
    private var pendingPermission: PermissionRequest? = null

    private val assetLoader by lazy {
        WebViewAssetLoader.Builder().addPathHandler("/assets/", WebViewAssetLoader.AssetsPathHandler(this)).build()
    }

    private val pickFiles = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        fileCallback?.onReceiveValue(WebChromeClient.FileChooserParams.parseResult(result.resultCode, result.data))
        fileCallback = null
    }

    private val askMicrophone = registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        pendingPermission?.let { if (granted) it.grant(arrayOf(PermissionRequest.RESOURCE_AUDIO_CAPTURE)) else it.deny() }
        pendingPermission = null
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val settings = Settings(this)
        if (settings.server == null) {
            startActivity(Intent(this, SettingsActivity::class.java))
            finish()
            return
        }
        windowId = synchronized(openWindows) {
            val id = savedInstanceState?.getString(EXTRA_WINDOW)
                ?: intent.getStringExtra(EXTRA_WINDOW)
                ?: WindowIds.next(openWindows)
            openWindows += id
            id
        }

        enableEdgeToEdge()
        web = WebView(this)
        val root = FrameLayout(this).apply { addView(web, ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT) }
        // The page fills the space between the status bar, the navigation bar
        // and the on-screen keyboard; Obsidian puts its editing toolbar at the
        // bottom of that space, right above the keyboard.
        ViewCompat.setOnApplyWindowInsetsListener(root) { view, insets ->
            val bars = insets.getInsets(
                WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout() or WindowInsetsCompat.Type.ime(),
            )
            view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
            WindowInsetsCompat.CONSUMED
        }
        setContentView(root)
        configureWebView()

        onBackPressedDispatcher.addCallback(this) {
            // Ignis turns Back into "close the open menu, drawer or dialog,
            // then go back in the note's history" through browser history.
            if (web.canGoBack()) {
                web.goBack()
            } else {
                isEnabled = false
                onBackPressedDispatcher.onBackPressed()
                isEnabled = true
            }
        }
        loadIfChanged()
    }

    override fun onResume() {
        super.onResume()
        // The server address may have been changed from another window.
        if (::web.isInitialized) loadIfChanged()
    }

    override fun onSaveInstanceState(outState: Bundle) {
        super.onSaveInstanceState(outState)
        if (::windowId.isInitialized) outState.putString(EXTRA_WINDOW, windowId)
    }

    override fun onDestroy() {
        if (::windowId.isInitialized && !isChangingConfigurations) {
            synchronized(openWindows) { openWindows -= windowId }
        }
        if (::web.isInitialized) web.destroy()
        super.onDestroy()
    }

    /** Opens the window's page if the server or vault differ from what is loaded. */
    private fun loadIfChanged() {
        val settings = Settings(this)
        val server = settings.server ?: return
        val wanted = server to settings.vault
        if (wanted == loaded) return
        loaded = wanted

        tabletScript?.remove()
        tabletScript = if (WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) {
            val script = assets.open("tablet.js").bufferedReader().use { it.readText() }
            WebViewCompat.addDocumentStartJavaScript(web, script, setOf(Urls.origin(server)))
        } else {
            null
        }
        val target = Urls.windowUrl(server, settings.vault, windowId, intent.getStringExtra(EXTRA_FILE))
        web.loadUrl(Urls.launcherUrl(target))
    }

    @SuppressLint("SetJavaScriptEnabled")
    private fun configureWebView() {
        web.settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = true
            mediaPlaybackRequiresUserGesture = false
            setSupportMultipleWindows(true)
            javaScriptCanOpenWindowsAutomatically = true
            allowFileAccess = false
            allowContentAccess = false
            mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
        }
        web.addJavascriptInterface(AppBridge(this), AppBridge.NAME)
        web.setDownloadListener { url, _, _, _, _ -> openExternal(url) }
        web.webViewClient = object : WebViewClient() {
            override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? =
                assetLoader.shouldInterceptRequest(request.url)

            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                val scheme = request.url.scheme
                // Like the desktop app, the window itself may go anywhere on the
                // web (login pages need that); other schemes go to their apps.
                if (scheme == "http" || scheme == "https") return false
                openExternal(request.url.toString())
                return true
            }
        }
        web.webChromeClient = object : WebChromeClient() {
            override fun onReceivedTitle(view: WebView, title: String?) {
                @Suppress("DEPRECATION")
                setTaskDescription(ActivityManager.TaskDescription(title ?: getString(R.string.app_name)))
            }

            override fun onCreateWindow(view: WebView, isDialog: Boolean, isUserGesture: Boolean, resultMsg: Message): Boolean {
                // The page wants a new window. A throwaway WebView catches the
                // URL it is going to load; the URL then decides what happens.
                val catcher = WebView(this@MainActivity)
                catcher.webViewClient = object : WebViewClient() {
                    override fun shouldOverrideUrlLoading(v: WebView, request: WebResourceRequest): Boolean {
                        handleNewWindow(request.url.toString())
                        v.destroy()
                        return true
                    }
                }
                (resultMsg.obj as WebView.WebViewTransport).webView = catcher
                resultMsg.sendToTarget()
                return true
            }

            override fun onShowFileChooser(view: WebView, callback: ValueCallback<Array<Uri>>, params: FileChooserParams): Boolean {
                fileCallback?.onReceiveValue(null)
                fileCallback = callback
                return try {
                    pickFiles.launch(params.createIntent())
                    true
                } catch (e: ActivityNotFoundException) {
                    fileCallback = null
                    false
                }
            }

            override fun onPermissionRequest(request: PermissionRequest) {
                // Only the microphone, and only for the configured server
                // (recording plugins such as Advanced Audio Recorder).
                val server = loaded?.first
                val fromServer = server != null && runCatching { Urls.sameOrigin(server, URI(request.origin.toString())) }.getOrDefault(false)
                if (!fromServer || request.resources.toList() != listOf(PermissionRequest.RESOURCE_AUDIO_CAPTURE)) {
                    request.deny()
                    return
                }
                if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) {
                    request.grant(request.resources)
                } else {
                    pendingPermission?.deny()
                    pendingPermission = request
                    askMicrophone.launch(Manifest.permission.RECORD_AUDIO)
                }
            }
        }
    }

    private fun handleNewWindow(url: String) {
        val server = loaded?.first ?: return
        when (val target = Urls.classifyNewWindow(server, url)) {
            is Urls.NewWindowTarget.Window -> openNewWindow(this, target.file)
            is Urls.NewWindowTarget.External -> openExternal(target.url)
            Urls.NewWindowTarget.Ignore -> {}
        }
    }

    private fun openExternal(url: String) {
        try {
            startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)).addCategory(Intent.CATEGORY_BROWSABLE))
        } catch (e: ActivityNotFoundException) {
            // Nothing on the device opens this kind of link.
        }
    }

    companion object {
        const val EXTRA_WINDOW = "dev.philaxis.ignis.WINDOW"
        const val EXTRA_FILE = "dev.philaxis.ignis.FILE"

        /** Ids of the windows alive in this process. */
        private val openWindows = mutableSetOf<String>()

        /**
         * Opens another window with its own workspace. On a large screen it
         * opens next to the current one, in split screen.
         */
        fun openNewWindow(context: Context, file: String? = null) {
            val intent = Intent(context, MainActivity::class.java)
                .addFlags(
                    Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_MULTIPLE_TASK or
                        Intent.FLAG_ACTIVITY_NEW_DOCUMENT or Intent.FLAG_ACTIVITY_LAUNCH_ADJACENT,
                )
            if (file != null) intent.putExtra(EXTRA_FILE, file)
            context.startActivity(intent)
        }
    }
}
