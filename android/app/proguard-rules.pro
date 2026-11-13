# The connect page calls these methods by name through addJavascriptInterface.
-keepclassmembers class dev.philaxis.ignis.AppBridge {
    @android.webkit.JavascriptInterface <methods>;
}
