package cz.jancejka.launchertagssync.ui.home

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import cz.jancejka.launchertagssync.R
import cz.jancejka.launchertagssync.ui.InfoCard
import cz.jancejka.launchertagssync.ui.UiState
import cz.jancejka.launchertagssync.ui.theme.LauncherTagsSyncTheme

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HomeScreen(
  state: UiState,
  onTwoWay: (Boolean) -> Unit,
  onStart: () -> Unit,
  onHistory: () -> Unit,
  onReset: () -> Unit,
) {
  var confirmReset by remember { mutableStateOf(false) }
  Scaffold(topBar = { CenterAlignedTopAppBar(title = { Text(stringResource(R.string.app_name)) }) }) { pad ->
    Column(
      Modifier.padding(pad).fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp),
      verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
      Text(stringResource(R.string.home_intro), style = MaterialTheme.typography.bodyLarge)
      Text(stringResource(R.string.mode_title), style = MaterialTheme.typography.titleMedium)
      Column(Modifier.selectableGroup(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        ModeOption(!state.twoWay, R.string.mode_one_way, R.string.mode_one_way_desc) { onTwoWay(false) }
        ModeOption(state.twoWay, R.string.mode_two_way, R.string.mode_two_way_desc) { onTwoWay(true) }
      }
      if (state.twoWay) {
        InfoCard {
          Text(stringResource(if (state.hasSyncState) R.string.state_present else R.string.state_none))
          if (state.hasSyncState) {
            TextButton(onClick = { confirmReset = true }) { Text(stringResource(R.string.state_reset)) }
          }
        }
      }
      Button(onClick = onStart, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.start)) }
      OutlinedButton(onClick = onHistory, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.history)) }
      Text(stringResource(R.string.about, state.coreVersion), style = MaterialTheme.typography.bodySmall)
    }
  }
  if (confirmReset) {
    AlertDialog(
      onDismissRequest = { confirmReset = false },
      title = { Text(stringResource(R.string.state_reset_confirm_title)) },
      text = { Text(stringResource(R.string.state_reset_confirm_text)) },
      confirmButton = {
        TextButton(onClick = {
          confirmReset = false
          onReset()
        }) { Text(stringResource(R.string.reset)) }
      },
      dismissButton = { TextButton(onClick = { confirmReset = false }) { Text(stringResource(R.string.cancel)) } },
    )
  }
}

@Composable
private fun ModeOption(selected: Boolean, title: Int, desc: Int, onClick: () -> Unit) {
  Row(
    Modifier.fillMaxWidth().selectable(selected = selected, onClick = onClick, role = Role.RadioButton).padding(4.dp),
    verticalAlignment = Alignment.Top,
  ) {
    RadioButton(selected = selected, onClick = null)
    Column(Modifier.padding(start = 12.dp)) {
      Text(stringResource(title), style = MaterialTheme.typography.titleSmall)
      Text(stringResource(desc), style = MaterialTheme.typography.bodyMedium)
    }
  }
}

@Preview(showBackground = true)
@Composable
private fun HomePreview() {
  LauncherTagsSyncTheme { HomeScreen(UiState(twoWay = true, hasSyncState = true, coreVersion = "0.1.0"), {}, {}, {}, {}) }
}
