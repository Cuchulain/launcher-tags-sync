package cz.jancejka.launchertagssync.ui

import android.content.Context
import android.content.Intent
import android.text.format.DateFormat
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import cz.jancejka.launchertagssync.R
import java.util.Date

/** Apps this tool exchanges backups with (release builds first). */
enum class PartnerApp(val label: String, val packages: List<String>) {
  Kvaesitso("Kvaesitso", listOf("de.mm20.launcher2.release", "de.mm20.launcher2.nightly", "de.mm20.launcher2.debug")),
  QuickSearch("Quick Search", listOf("com.tk.quicksearch", "com.tk.quicksearch.debug"));

  fun launchIntent(context: Context): Intent? =
    packages.firstNotNullOfOrNull { context.packageManager.getLaunchIntentForPackage(it) }
}

@Composable
fun OpenAppButton(app: PartnerApp, modifier: Modifier = Modifier) {
  val context = LocalContext.current
  val intent = app.launchIntent(context)
  if (intent == null) {
    Text(stringResource(R.string.app_not_installed, app.label), style = MaterialTheme.typography.bodySmall, modifier = modifier)
  } else {
    OutlinedButton(onClick = { context.startActivity(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }, modifier = modifier) {
      Text(stringResource(R.string.open_app, app.label))
    }
  }
}

@Composable
fun InfoCard(modifier: Modifier = Modifier, container: Color? = null, content: @Composable () -> Unit) {
  Card(
    modifier = modifier.fillMaxWidth(),
    colors =
      if (container != null) CardDefaults.cardColors(containerColor = container) else CardDefaults.cardColors(),
  ) {
    Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) { content() }
  }
}

@Composable
fun formatTime(ms: Long): String {
  val context = LocalContext.current
  val d = Date(ms)
  return DateFormat.getMediumDateFormat(context).format(d) + " " + DateFormat.getTimeFormat(context).format(d)
}
