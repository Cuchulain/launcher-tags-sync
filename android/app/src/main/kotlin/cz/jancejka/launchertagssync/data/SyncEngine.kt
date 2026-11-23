package cz.jancejka.launchertagssync.data

import uniffi.launcher_tags_ffi.BothResult
import uniffi.launcher_tags_ffi.KvaesitsoInfo
import uniffi.launcher_tags_ffi.OneWayResult
import uniffi.launcher_tags_ffi.PendingMode
import uniffi.launcher_tags_ffi.QuickSearchInfo

/**
 * The conversion engine. The production implementation calls the Rust core
 * (launcher-tags-core via UniFFI); tests use a fake.
 *
 * All methods are blocking and CPU-bound: call them off the main thread.
 * They throw [uniffi.launcher_tags_ffi.SyncException] on invalid input or refused syncs.
 */
interface SyncEngine {
  fun coreVersion(): String

  fun inspectKvaesitso(bytes: ByteArray): KvaesitsoInfo

  fun inspectQuickSearch(bytes: ByteArray): QuickSearchInfo

  fun oneWay(kvaesitso: ByteArray, quicksearch: ByteArray): OneWayResult

  fun both(
    kvaesitso: ByteArray,
    quicksearch: ByteArray,
    state: ByteArray?,
    pending: PendingMode,
    allowMassDelete: Boolean,
    allowStale: Boolean,
  ): BothResult
}

object RustSyncEngine : SyncEngine {
  override fun coreVersion(): String = uniffi.launcher_tags_ffi.coreVersion()

  override fun inspectKvaesitso(bytes: ByteArray) = uniffi.launcher_tags_ffi.inspectKvaesitso(bytes)

  override fun inspectQuickSearch(bytes: ByteArray) = uniffi.launcher_tags_ffi.inspectQuicksearch(bytes)

  override fun oneWay(kvaesitso: ByteArray, quicksearch: ByteArray) =
    uniffi.launcher_tags_ffi.syncOneWay(kvaesitso, quicksearch)

  override fun both(
    kvaesitso: ByteArray,
    quicksearch: ByteArray,
    state: ByteArray?,
    pending: PendingMode,
    allowMassDelete: Boolean,
    allowStale: Boolean,
  ) = uniffi.launcher_tags_ffi.syncBoth(kvaesitso, quicksearch, state, pending, allowMassDelete, allowStale)
}
