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
/// The numbers are offered rather than pushed. Pushing them means running a
/// script in whatever document happens to be loaded, and the one loaded when
/// the insets first arrive is the empty one before the program's own page:
/// the properties were being set on a document that was about to be thrown
/// away. Bound to the view instead, they survive every navigation and the
/// page reads them whenever it likes.
class MainActivity : TauriActivity() {
  private var top = 0f
  private var bottom = 0f
  private var left = 0f
  private var right = 0f

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
      // Returned rather than dispatched onwards: handing them back to the
      // same view runs this listener again, and the loop takes the main
      // thread with it.
      insets
    }
  }

  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)
    // Only this program's own page is ever loaded here, and all this offers
    // is four numbers about the screen.
    webView.addJavascriptInterface(Insets(), "tsuburuInsets")
  }

  /// Read from the page as `window.tsuburuInsets.bottom()`, in CSS pixels.
  inner class Insets {
    @JavascriptInterface fun top(): Float = top
    @JavascriptInterface fun bottom(): Float = bottom
    @JavascriptInterface fun left(): Float = left
    @JavascriptInterface fun right(): Float = right
  }
}
