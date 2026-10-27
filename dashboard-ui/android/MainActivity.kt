package dev.dioxus.main

import android.content.res.Configuration
import android.os.Bundle
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import kotlin.math.roundToInt

typealias BuildConfig = cc.attodao.ahd.BuildConfig

// tao/wry cannot survive a destroyed Activity: the Rust event loop ends and the process
// aborts ("pthread_mutex_lock called on a destroyed mutex"), and a second instance aborts
// in create(). So there is only ever one instance (launchMode singleTask, set with the
// manifest's configChanges by scripts/build-android.sh), it is never finished or recreated
// on purpose (Back on the first history entry moves the app to the background like Home,
// configuration changes are applied in place), and when the system destroys it anyway
// (task swiped away, unhandled change) the process ends without running native teardown.
class MainActivity : WryActivity() {
    private var webView: WebView? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // WryActivity.onKeyDown still goes back in the WebView history first.
        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() {
                moveTaskToBack(true)
            }
        })
    }

    override fun onWebViewCreate(webView: WebView) {
        this.webView = webView
    }

    override fun onDestroy() {
        // SIGKILL instead of tao's exit(): Android starts a fresh process when the app is
        // opened again.
        android.os.Process.killProcess(android.os.Process.myPid())
        super.onDestroy()
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        // The WebView takes the system font size only when it is created.
        webView?.settings?.textZoom = (newConfig.fontScale * 100).roundToInt()
    }
}
