package la.tsuburu.tsuburu

import android.os.Bundle
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

/// The window draws under the system bars, so the page has to know where they
/// are. It learns the top one on its own - `env(safe-area-inset-top)` comes
/// back as the status bar's height - but the bottom one is measured as zero
/// however the gesture bar is drawn, and the tab bar ends up underneath it.
///
/// They are both offered and sent, because either one alone loses a race.
///
/// Sending means running a script in whatever document is loaded, and when
/// the insets first arrive that is often the empty one before the program's
/// own page - so the properties are set on a document about to be thrown
/// away. Offering alone fails the other way round: a page that loads before
/// the first insets arrive reads zero and has nothing to tell it otherwise,
/// and a bar at the top of the window ends up under the status bar.
///
/// So the page reads them when it loads, which covers insets that came
/// first, and they are sent on every change, which covers the ones that come
/// after - including turning the phone over.
class MainActivity : TauriActivity() {
  private var top = 0f
  private var bottom = 0f
  private var left = 0f
  private var right = 0f
  private var webView: WebView? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)

    ViewCompat.setOnApplyWindowInsetsListener(window.decorView) { _, insets ->
      val bars = insets.getInsets(
        WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
      )
      val density = resources.displayMetrics.density
      top = bars.top / density
      bottom = bars.bottom / density
      left = bars.left / density
      right = bars.right / density
      send()
      // Returned rather than dispatched onwards: handing them back to the
      // same view runs this listener again, and the loop takes the main
      // thread with it.
      insets
    }
  }

  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)
    this.webView = webView
    // Only this program's own page is ever loaded here, and all this offers
    // is four numbers about the screen.
    webView.addJavascriptInterface(Insets(), "tsuburuInsets")
    send()
  }

  private fun send() {
    val view = webView ?: return
    val js =
      "(function(){var s=document.documentElement.style;" +
        "s.setProperty('--inset-top','${top}px');" +
        "s.setProperty('--inset-bottom','${bottom}px');" +
        "s.setProperty('--inset-left','${left}px');" +
        "s.setProperty('--inset-right','${right}px');})();"
    view.post { view.evaluateJavascript(js, null) }
  }

  /// Read from the page as `window.tsuburuInsets.bottom()`, in CSS pixels.
  inner class Insets {
    @JavascriptInterface fun top(): Float = top
    @JavascriptInterface fun bottom(): Float = bottom
    @JavascriptInterface fun left(): Float = left
    @JavascriptInterface fun right(): Float = right
  }
}
