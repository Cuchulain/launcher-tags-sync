package cz.jancejka.launchertagssync.ui.history

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import cz.jancejka.launchertagssync.R
import cz.jancejka.launchertagssync.data.HistoryEntry
import cz.jancejka.launchertagssync.ui.InfoCard
import cz.jancejka.launchertagssync.ui.formatTime

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HistoryScreen(history: List<HistoryEntry>, onBack: () -> Unit) {
  Scaffold(
    topBar = {
      TopAppBar(
        title = { Text(stringResource(R.string.history)) },
        navigationIcon = {
          IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.back)) }
        },
      )
    }
  ) { pad ->
    if (history.isEmpty()) {
      Text(stringResource(R.string.history_empty), Modifier.padding(pad).padding(16.dp))
    } else {
      LazyColumn(
        Modifier.padding(pad),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
      ) {
        items(history) { e ->
          InfoCard {
            val mode =
              when {
                !e.twoWay -> R.string.history_one_way
                e.initial -> R.string.history_two_way_initial
                else -> R.string.history_two_way
              }
            Text(formatTime(e.timeMs) + " · " + stringResource(mode), style = MaterialTheme.typography.titleSmall)
            Text(
              stringResource(
                R.string.history_line,
                e.quickSearchEntriesChanged,
                e.kvaesitsoPackagesChanged,
                e.aliasesAdded,
                e.deletions,
                e.conflicts,
              )
            )
          }
        }
      }
    }
  }
}
