package cz.jancejka.launchertagssync

import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.ui.Modifier
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import cz.jancejka.launchertagssync.data.ContentResolverIo
import cz.jancejka.launchertagssync.data.RustSyncEngine
import cz.jancejka.launchertagssync.data.SyncStore
import cz.jancejka.launchertagssync.ui.AppViewModel
import cz.jancejka.launchertagssync.ui.theme.LauncherTagsSyncTheme

class MainActivity : ComponentActivity() {
  private val vm: AppViewModel by viewModels { factory() }

  private fun factory(): ViewModelProvider.Factory = viewModelFactory {
    initializer {
      AppViewModel(
        engine = RustSyncEngine,
        io = ContentResolverIo(applicationContext.contentResolver),
        store = SyncStore(applicationContext.filesDir),
      )
    }
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    if (savedInstanceState == null) handleShare(intent)
    enableEdgeToEdge()
    setContent {
      LauncherTagsSyncTheme {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) { MainNavigation(vm) }
      }
    }
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    handleShare(intent)
  }

  /** A backup shared from Quick Search / Kvaesitso / a file manager via "Share". */
  private fun handleShare(intent: Intent?) {
    if (intent?.action != Intent.ACTION_SEND) return
    val uri: Uri? =
      if (Build.VERSION.SDK_INT >= 33) intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
      else @Suppress("DEPRECATION") intent.getParcelableExtra(Intent.EXTRA_STREAM)
    uri?.let { vm.receiveShared(it.toString()) }
  }
}
