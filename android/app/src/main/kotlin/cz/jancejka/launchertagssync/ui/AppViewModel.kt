package cz.jancejka.launchertagssync.ui

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import cz.jancejka.launchertagssync.data.DocumentIo
import cz.jancejka.launchertagssync.data.HistoryEntry
import cz.jancejka.launchertagssync.data.SyncEngine
import cz.jancejka.launchertagssync.data.SyncStore
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.launcher_tags_ffi.BothResult
import uniffi.launcher_tags_ffi.ErrorKind
import uniffi.launcher_tags_ffi.KvaesitsoInfo
import uniffi.launcher_tags_ffi.OneWayResult
import uniffi.launcher_tags_ffi.PendingMode
import uniffi.launcher_tags_ffi.QuickSearchInfo
import uniffi.launcher_tags_ffi.SyncException

enum class Step {
  ExportKvaesitso,
  ExportQuickSearch,
  Preview,
  Save,
  Import,
}

data class PickedBackup<I>(val name: String, val bytes: ByteArray, val info: I) {
  override fun equals(other: Any?) = other is PickedBackup<*> && other.name == name && other.bytes.contentEquals(bytes)

  override fun hashCode() = name.hashCode() * 31 + bytes.contentHashCode()
}

/** An error to show; [kind] is null for I/O and unexpected errors. */
data class UiError(val kind: ErrorKind?, val detail: String)

sealed interface SyncPreview {
  data class OneWay(val result: OneWayResult) : SyncPreview

  data class Both(val result: BothResult) : SyncPreview
}

/** Overrides the user explicitly chose after a refusal. */
data class Overrides(
  val pending: PendingMode = PendingMode.STOP,
  val allowMassDelete: Boolean = false,
  val allowStale: Boolean = false,
)

data class UiState(
  val twoWay: Boolean = false,
  val hasSyncState: Boolean = false,
  val history: List<HistoryEntry> = emptyList(),
  val coreVersion: String = "",
  /** Incremented when something (e.g. a shared file) asks the UI to show the wizard. */
  val openWizardRequests: Int = 0,
  // wizard
  val step: Step = Step.ExportKvaesitso,
  val kvaesitso: PickedBackup<KvaesitsoInfo>? = null,
  val quickSearch: PickedBackup<QuickSearchInfo>? = null,
  val busy: Boolean = false,
  val error: UiError? = null,
  val overrides: Overrides = Overrides(),
  val preview: SyncPreview? = null,
  val savedQuickSearch: Boolean = false,
  val savedKvaesitso: Boolean = false,
  val committed: Boolean = false,
) {
  val needsQuickSearchSave: Boolean
    get() =
      when (val p = preview) {
        is SyncPreview.OneWay -> true
        is SyncPreview.Both -> p.result.toQuicksearch.isNotEmpty()
        null -> false
      }

  val needsKvaesitsoSave: Boolean
    get() = (preview as? SyncPreview.Both)?.result?.toKvaesitso?.isNotEmpty() == true

  val hasChanges: Boolean
    get() = needsQuickSearchSave || needsKvaesitsoSave

  val allSaved: Boolean
    get() = (!needsQuickSearchSave || savedQuickSearch) && (!needsKvaesitsoSave || savedKvaesitso)
}

class AppViewModel(
  private val engine: SyncEngine,
  private val io: DocumentIo,
  private val store: SyncStore,
  private val worker: CoroutineDispatcher = Dispatchers.Default,
  private val clock: () -> Long = System::currentTimeMillis,
) : ViewModel() {
  private val _state = MutableStateFlow(UiState())
  val state: StateFlow<UiState> = _state.asStateFlow()

  init {
    refreshHome()
    viewModelScope.launch {
      val v = runCatching { withContext(worker) { engine.coreVersion() } }.getOrDefault("?")
      _state.update { it.copy(coreVersion = v) }
    }
  }

  private fun refreshHome() {
    _state.update { it.copy(twoWay = store.twoWay, hasSyncState = store.hasState(), history = store.history()) }
  }

  fun setTwoWay(twoWay: Boolean) {
    store.twoWay = twoWay
    _state.update { it.copy(twoWay = twoWay, preview = null) }
  }

  /** Start a fresh wizard run (keeps the chosen mode). */
  fun startWizard() =
    _state.update {
      UiState(twoWay = it.twoWay, hasSyncState = store.hasState(), history = it.history, coreVersion = it.coreVersion)
    }

  fun resetSyncState() {
    store.resetState()
    refreshHome()
  }

  fun dismissError() = _state.update { it.copy(error = null) }

  fun goTo(step: Step) {
    _state.update { it.copy(step = step, error = null) }
    if (step == Step.Preview) computePreview()
  }

  fun back() {
    val s = _state.value
    val prev = Step.entries.getOrNull(s.step.ordinal - 1) ?: return
    // Never step back once outputs were saved: re-planning would not match them.
    if (s.savedQuickSearch || s.savedKvaesitso) return
    _state.update { it.copy(step = prev, error = null) }
  }

  /**
   * A backup shared to this app (Android "Share"): detect its type, put it into the wizard
   * (starting a fresh one if the current run already saved outputs) and ask the UI to show it.
   */
  fun receiveShared(uri: String) {
    if (_state.value.let { it.savedQuickSearch || it.savedKvaesitso || it.committed }) startWizard()
    _state.update { it.copy(openWizardRequests = it.openWizardRequests + 1) }
    load({
      val doc = io.read(uri)
      when (detectBackupType(doc.bytes)) {
        BackupType.QuickSearch -> PickedBackup(doc.name, doc.bytes, engine.inspectQuickSearch(doc.bytes))
        BackupType.Kvaesitso -> PickedBackup(doc.name, doc.bytes, engine.inspectKvaesitso(doc.bytes))
        BackupType.Unknown -> throw SyncException.Failed(ErrorKind.INVALID_INPUT, "not a Quick Search or Kvaesitso backup")
      }
    }) { picked ->
      @Suppress("UNCHECKED_CAST")
      val next =
        when (picked.info) {
          is QuickSearchInfo -> copy(quickSearch = picked as PickedBackup<QuickSearchInfo>, preview = null)
          else -> copy(kvaesitso = picked as PickedBackup<KvaesitsoInfo>, preview = null)
        }
      next.copy(
        step =
          when {
            next.kvaesitso == null -> Step.ExportKvaesitso
            else -> Step.ExportQuickSearch
          }
      )
    }
  }

  fun pickKvaesitso(uri: String) =
    load({ io.read(uri).let { PickedBackup(it.name, it.bytes, engine.inspectKvaesitso(it.bytes)) } }) {
      copy(kvaesitso = it, preview = null)
    }

  fun pickQuickSearch(uri: String) =
    load({ io.read(uri).let { PickedBackup(it.name, it.bytes, engine.inspectQuickSearch(it.bytes)) } }) {
      copy(quickSearch = it, preview = null)
    }

  /** Run [block] on the worker, then fold its result into the state (or show the error). */
  private fun <T> load(block: () -> T, apply: UiState.(T) -> UiState) {
    _state.update { it.copy(busy = true, error = null) }
    viewModelScope.launch {
      val result = runCatching { withContext(worker) { block() } }
      _state.update { st ->
        result.fold(
          onSuccess = { st.apply(it).copy(busy = false) },
          onFailure = { st.copy(busy = false, error = it.toUiError()) },
        )
      }
    }
  }

  fun applyOverride(change: (Overrides) -> Overrides) {
    _state.update { it.copy(overrides = change(it.overrides)) }
    computePreview()
  }

  fun computePreview() {
    val s = _state.value
    val k = s.kvaesitso ?: return
    val q = s.quickSearch ?: return
    _state.update { it.copy(busy = true, error = null, preview = null) }
    viewModelScope.launch {
      val result =
        runCatching {
          withContext(worker) {
            if (s.twoWay) {
              SyncPreview.Both(
                engine.both(
                  k.bytes,
                  q.bytes,
                  store.readState(),
                  s.overrides.pending,
                  s.overrides.allowMassDelete,
                  s.overrides.allowStale,
                )
              )
            } else {
              SyncPreview.OneWay(engine.oneWay(k.bytes, q.bytes))
            }
          }
        }
      _state.update { st ->
        result.fold(
          onSuccess = { st.copy(busy = false, preview = it) },
          onFailure = { st.copy(busy = false, error = it.toUiError()) },
        )
      }
    }
  }

  fun saveQuickSearch(uri: String) = save(uri, isKvaesitso = false)

  fun saveKvaesitso(uri: String) = save(uri, isKvaesitso = true)

  private fun save(uri: String, isKvaesitso: Boolean) {
    val preview = _state.value.preview ?: return
    val bytes =
      when (preview) {
        is SyncPreview.OneWay -> if (isKvaesitso) return else preview.result.quicksearch
        is SyncPreview.Both -> if (isKvaesitso) preview.result.kvaesitso else preview.result.quicksearch
      }
    _state.update { it.copy(busy = true, error = null) }
    viewModelScope.launch {
      val r = runCatching { withContext(worker) { io.write(uri, bytes) } }
      _state.update {
        r.fold(
          onSuccess = { _ -> if (isKvaesitso) it.copy(busy = false, savedKvaesitso = true) else it.copy(busy = false, savedQuickSearch = true) },
          onFailure = { e -> it.copy(busy = false, error = e.toUiError()) },
        )
      }
      commitIfComplete()
    }
  }

  /**
   * Once every needed output is saved: persist the new sync state (two-way) and a history
   * entry. Also used when there is nothing to save.
   */
  fun commitIfComplete() {
    val s = _state.value
    val preview = s.preview ?: return
    if (s.committed || !s.allSaved) return
    val entry =
      when (preview) {
        is SyncPreview.OneWay ->
          HistoryEntry(
            timeMs = clock(),
            twoWay = false,
            initial = false,
            quickSearchEntriesChanged = preview.result.changes.size,
            kvaesitsoPackagesChanged = 0,
            aliasesAdded = preview.result.addedAliases.toInt(),
            deletions = 0,
            conflicts = 0,
          )
        is SyncPreview.Both -> {
          store.commitState(preview.result.state)
          HistoryEntry(
            timeMs = clock(),
            twoWay = true,
            initial = preview.result.initial,
            quickSearchEntriesChanged = preview.result.toQuicksearch.size,
            kvaesitsoPackagesChanged = preview.result.toKvaesitso.size,
            aliasesAdded =
              (preview.result.toQuicksearch + preview.result.toKvaesitso).sumOf { it.added.size },
            deletions = preview.result.deletions.toInt(),
            conflicts = preview.result.conflicts.size,
          )
        }
      }
    store.addHistory(entry)
    _state.update { it.copy(committed = true, hasSyncState = store.hasState(), history = store.history()) }
  }
}

fun Throwable.toUiError(): UiError =
  when (this) {
    is SyncException.Failed -> UiError(kind, reason)
    else -> UiError(null, message ?: javaClass.simpleName)
  }

enum class BackupType {
  QuickSearch,
  Kvaesitso,
  Unknown,
}

/** Quick Search backups are JSON objects, Kvaesitso backups are ZIP archives. */
fun detectBackupType(bytes: ByteArray): BackupType {
  val first = bytes.firstOrNull { !it.toInt().toChar().isWhitespace() && it != 0xEF.toByte() && it != 0xBB.toByte() && it != 0xBF.toByte() }
  return when {
    bytes.size >= 4 && bytes[0] == 'P'.code.toByte() && bytes[1] == 'K'.code.toByte() -> BackupType.Kvaesitso
    first == '{'.code.toByte() -> BackupType.QuickSearch
    else -> BackupType.Unknown
  }
}
