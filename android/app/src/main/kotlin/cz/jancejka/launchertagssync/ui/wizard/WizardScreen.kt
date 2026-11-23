package cz.jancejka.launchertagssync.ui.wizard

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import cz.jancejka.launchertagssync.R
import cz.jancejka.launchertagssync.ui.AppViewModel
import cz.jancejka.launchertagssync.ui.InfoCard
import cz.jancejka.launchertagssync.ui.OpenAppButton
import cz.jancejka.launchertagssync.ui.PartnerApp
import cz.jancejka.launchertagssync.ui.Step
import cz.jancejka.launchertagssync.ui.SyncPreview
import cz.jancejka.launchertagssync.ui.UiError
import cz.jancejka.launchertagssync.ui.UiState
import cz.jancejka.launchertagssync.ui.formatTime
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import uniffi.launcher_tags_ffi.ErrorKind
import uniffi.launcher_tags_ffi.PackageChange
import uniffi.launcher_tags_ffi.PendingMode

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun WizardScreen(state: UiState, vm: AppViewModel, onClose: () -> Unit) {
  val steps = Step.entries
  Scaffold(
    topBar = {
      TopAppBar(
        title = { Text(stringResource(R.string.step_of, state.step.ordinal + 1, steps.size)) },
        navigationIcon = { IconButton(onClick = onClose) { Icon(Icons.Filled.Close, stringResource(R.string.cancel)) } },
      )
    },
    bottomBar = { NavBar(state, vm, onClose) },
  ) { pad ->
    Column(Modifier.padding(pad).fillMaxSize()) {
      LinearProgressIndicator(
        progress = { (state.step.ordinal + 1f) / steps.size },
        modifier = Modifier.fillMaxWidth(),
      )
      Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
      ) {
        when (state.step) {
          Step.ExportKvaesitso -> ExportKvaesitso(state, vm)
          Step.ExportQuickSearch -> ExportQuickSearch(state, vm)
          Step.Preview -> Preview(state, vm)
          Step.Save -> Save(state, vm)
          Step.Import -> Import(state)
        }
        if (state.busy) {
          Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
        }
        state.error?.let { ErrorCard(it, state, vm) }
      }
    }
  }
}

@Composable
private fun NavBar(state: UiState, vm: AppViewModel, onClose: () -> Unit) {
  val canNext =
    !state.busy &&
      when (state.step) {
        Step.ExportKvaesitso -> state.kvaesitso != null
        Step.ExportQuickSearch -> state.quickSearch != null
        Step.Preview -> state.preview != null
        Step.Save -> state.allSaved
        Step.Import -> true
      }
  Row(
    Modifier.fillMaxWidth().padding(16.dp),
    horizontalArrangement = Arrangement.SpaceBetween,
  ) {
    val locked = state.savedKvaesitso || state.savedQuickSearch
    TextButton(onClick = vm::back, enabled = state.step != Step.ExportKvaesitso && !locked && !state.busy) {
      Text(stringResource(R.string.back))
    }
    if (state.step == Step.Import) {
      Button(onClick = onClose) { Text(stringResource(R.string.done)) }
    } else {
      Button(
        enabled = canNext,
        onClick = {
          if (state.step == Step.Save) vm.commitIfComplete()
          vm.goTo(Step.entries[state.step.ordinal + 1])
        },
      ) { Text(stringResource(R.string.next)) }
    }
  }
}

@Composable
private fun ExportKvaesitso(state: UiState, vm: AppViewModel) {
  val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
    uri?.let { vm.pickKvaesitso(it.toString()) }
  }
  Text(stringResource(R.string.export_k_title), style = MaterialTheme.typography.headlineSmall)
  Text(stringResource(R.string.export_k_steps))
  OpenAppButton(PartnerApp.Kvaesitso)
  Button(onClick = { picker.launch(arrayOf("*/*")) }, enabled = !state.busy) {
    Text(stringResource(if (state.kvaesitso == null) R.string.pick_file else R.string.pick_other_file))
  }
  state.kvaesitso?.let { k ->
    InfoCard {
      Text(
        stringResource(
          R.string.k_info,
          k.name,
          k.info.appTagRecords.toInt(),
          k.info.appPackages.toInt(),
          k.info.pages.toInt(),
          k.info.shortcutTagsSkipped.toInt(),
        )
      )
      Text(stringResource(R.string.format_line, k.info.format ?: "?", k.info.appVersion ?: "?"), style = MaterialTheme.typography.bodySmall)
      k.info.exportedAtMs?.let { Text(stringResource(R.string.exported_at, formatTime(it)), style = MaterialTheme.typography.bodySmall) }
      Warnings(k.info.warnings)
    }
  }
}

@Composable
private fun ExportQuickSearch(state: UiState, vm: AppViewModel) {
  val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
    uri?.let { vm.pickQuickSearch(it.toString()) }
  }
  Text(stringResource(R.string.export_q_title), style = MaterialTheme.typography.headlineSmall)
  Text(stringResource(R.string.export_q_steps))
  OpenAppButton(PartnerApp.QuickSearch)
  Button(onClick = { picker.launch(arrayOf("*/*")) }, enabled = !state.busy) {
    Text(stringResource(if (state.quickSearch == null) R.string.pick_file else R.string.pick_other_file))
  }
  state.quickSearch?.let { q ->
    InfoCard {
      Text(stringResource(R.string.q_info, q.name, q.info.appNicknames.toInt(), q.info.profileAppNicknames.toInt()))
      q.info.exportedAtMs?.let { Text(stringResource(R.string.exported_at, formatTime(it)), style = MaterialTheme.typography.bodySmall) }
    }
  }
}

@Composable
private fun Preview(state: UiState, vm: AppViewModel) {
  LaunchedEffect(state.kvaesitso, state.quickSearch, state.twoWay) {
    if (state.preview == null && !state.busy && state.error == null) vm.computePreview()
  }
  Text(stringResource(R.string.preview_title), style = MaterialTheme.typography.headlineSmall)
  when (val p = state.preview) {
    null -> {}
    is SyncPreview.OneWay -> {
      val r = p.result
      if (r.changes.isEmpty()) {
        Text(stringResource(R.string.preview_nothing))
      } else {
        Text(stringResource(R.string.preview_summary_one_way, r.newEntries.toInt(), r.extendedEntries.toInt(), r.addedAliases.toInt()))
        Changes(stringResource(R.string.preview_to_q, r.changes.size), r.changes)
      }
      Warnings(r.warnings)
    }
    is SyncPreview.Both -> {
      val r = p.result
      if (r.initial) Text(stringResource(R.string.preview_initial))
      r.pendingNote?.let { Text(stringResource(R.string.pending_note, it)) }
      if (r.toQuicksearch.isEmpty() && r.toKvaesitso.isEmpty()) Text(stringResource(R.string.preview_nothing))
      if (r.deletions > 0u) {
        Text(stringResource(R.string.preview_deletions, r.deletions.toInt()), color = MaterialTheme.colorScheme.error)
      }
      if (r.toQuicksearch.isNotEmpty()) Changes(stringResource(R.string.preview_to_q, r.toQuicksearch.size), r.toQuicksearch)
      if (r.toKvaesitso.isNotEmpty()) Changes(stringResource(R.string.preview_to_k, r.toKvaesitso.size), r.toKvaesitso)
      if (r.conflicts.isNotEmpty()) {
        InfoCard(container = MaterialTheme.colorScheme.tertiaryContainer) {
          Text(stringResource(R.string.conflicts), style = MaterialTheme.typography.titleSmall)
          r.conflicts.forEach { Text("• $it") }
        }
      }
      if (r.unresolvableApps.isNotEmpty()) {
        Text(stringResource(R.string.unresolvable, r.unresolvableApps.size), style = MaterialTheme.typography.bodySmall)
      }
      Warnings(r.warnings)
    }
  }
}

@Composable
private fun Changes(title: String, changes: List<PackageChange>) {
  var expanded by rememberSaveable(title) { mutableStateOf(false) }
  InfoCard {
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
      Text(title, style = MaterialTheme.typography.titleSmall)
      TextButton(onClick = { expanded = !expanded }) { Text(if (expanded) "▲" else "▼") }
    }
    val shown = if (expanded) changes else changes.take(5)
    shown.forEach { c ->
      HorizontalDivider()
      Text(
        c.`package` + "  " + stringResource(R.string.change_line, c.added.size, c.removed.size) +
          if (c.newEntry) " (" + stringResource(R.string.new_entry) + ")" else "",
        style = MaterialTheme.typography.bodyMedium,
      )
      if (expanded) {
        if (c.added.isNotEmpty()) Text(stringResource(R.string.added, c.added.joinToString(", ")), style = MaterialTheme.typography.bodySmall)
        if (c.removed.isNotEmpty()) Text(stringResource(R.string.removed, c.removed.joinToString(", ")), style = MaterialTheme.typography.bodySmall)
      }
    }
    if (!expanded && changes.size > shown.size) Text("… +${changes.size - shown.size}", style = MaterialTheme.typography.bodySmall)
  }
}

@Composable
private fun Warnings(warnings: List<String>) {
  if (warnings.isEmpty()) return
  var expanded by rememberSaveable { mutableStateOf(false) }
  TextButton(onClick = { expanded = !expanded }) { Text(stringResource(R.string.warnings) + " (${warnings.size})") }
  if (expanded) warnings.forEach { Text("• $it", style = MaterialTheme.typography.bodySmall) }
}

@Composable
private fun ErrorCard(error: UiError, state: UiState, vm: AppViewModel) {
  InfoCard(container = MaterialTheme.colorScheme.errorContainer) {
    Text(stringResource(R.string.error_title), style = MaterialTheme.typography.titleSmall)
    val explanation =
      when (error.kind) {
        ErrorKind.STALE_INPUT -> R.string.error_stale
        ErrorKind.PENDING_NOT_IMPORTED -> R.string.error_pending
        ErrorKind.MASS_DELETE -> R.string.error_mass_delete
        ErrorKind.SETTINGS_NOT_EXPORTED -> R.string.error_settings
        ErrorKind.TYPE_COLLISION -> R.string.error_type
        ErrorKind.INVALID_INPUT -> R.string.error_invalid
        ErrorKind.STATE -> R.string.error_state
        ErrorKind.INTERNAL, null -> R.string.error_other
      }
    Text(stringResource(explanation))
    Text(error.detail, style = MaterialTheme.typography.bodySmall)
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
      when (error.kind) {
        ErrorKind.STALE_INPUT ->
          OutlinedButton(onClick = { vm.applyOverride { it.copy(allowStale = true) } }) { Text(stringResource(R.string.action_use_anyway)) }
        ErrorKind.PENDING_NOT_IMPORTED -> {
          OutlinedButton(onClick = { vm.applyOverride { it.copy(pending = PendingMode.DISCARD) } }) { Text(stringResource(R.string.action_discard)) }
          OutlinedButton(onClick = { vm.applyOverride { it.copy(pending = PendingMode.ACCEPT) } }) { Text(stringResource(R.string.action_accept)) }
        }
        ErrorKind.MASS_DELETE ->
          OutlinedButton(onClick = { vm.applyOverride { it.copy(allowMassDelete = true) } }) { Text(stringResource(R.string.action_allow_delete)) }
        else -> TextButton(onClick = vm::dismissError) { Text(stringResource(R.string.dismiss)) }
      }
    }
  }
}

@Composable
private fun Save(state: UiState, vm: AppViewModel) {
  val stamp = SimpleDateFormat("yyyyMMdd-HHmm", Locale.ROOT).format(Date())
  val saveQ = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { uri ->
    uri?.let { vm.saveQuickSearch(it.toString()) }
  }
  val saveK = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { uri ->
    uri?.let { vm.saveKvaesitso(it.toString()) }
  }
  Text(stringResource(R.string.save_title), style = MaterialTheme.typography.headlineSmall)
  if (!state.hasChanges) {
    Text(stringResource(R.string.save_nothing))
    return
  }
  Text(stringResource(R.string.save_text))
  if (state.needsQuickSearchSave) {
    SaveRow(R.string.save_q, state.savedQuickSearch, state.busy) { saveQ.launch("synced-$stamp.quicksearch") }
  }
  if (state.needsKvaesitsoSave) {
    SaveRow(R.string.save_k, state.savedKvaesitso, state.busy) { saveK.launch("synced-$stamp.kvaesitso") }
  }
}

@Composable
private fun SaveRow(label: Int, saved: Boolean, busy: Boolean, onSave: () -> Unit) {
  Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
    Button(onClick = onSave, enabled = !busy) { Text(stringResource(label)) }
    if (saved) Text(stringResource(R.string.saved), color = MaterialTheme.colorScheme.primary)
  }
}

@Composable
private fun Import(state: UiState) {
  Text(stringResource(R.string.import_title), style = MaterialTheme.typography.headlineSmall)
  if (!state.hasChanges) {
    Text(stringResource(R.string.import_nothing))
    return
  }
  if (state.needsQuickSearchSave) {
    InfoCard {
      Text(stringResource(R.string.import_q))
      OpenAppButton(PartnerApp.QuickSearch)
    }
  }
  if (state.needsKvaesitsoSave) {
    InfoCard {
      Text(stringResource(R.string.import_k))
      OpenAppButton(PartnerApp.Kvaesitso)
    }
  }
  if (state.needsQuickSearchSave && state.needsKvaesitsoSave) Text(stringResource(R.string.import_both_needed))
}
