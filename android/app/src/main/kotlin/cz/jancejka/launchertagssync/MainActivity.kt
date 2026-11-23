package cz.jancejka.launchertagssync

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
    enableEdgeToEdge()
    setContent {
      LauncherTagsSyncTheme {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) { MainNavigation(vm) }
      }
    }
  }
}
