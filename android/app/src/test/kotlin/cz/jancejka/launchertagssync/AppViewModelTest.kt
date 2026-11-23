package cz.jancejka.launchertagssync

import cz.jancejka.launchertagssync.data.DocumentIo
import cz.jancejka.launchertagssync.data.SyncEngine
import cz.jancejka.launchertagssync.data.SyncStore
import cz.jancejka.launchertagssync.ui.AppViewModel
import cz.jancejka.launchertagssync.ui.Step
import cz.jancejka.launchertagssync.ui.SyncPreview
import java.io.File
import java.nio.file.Files
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import uniffi.launcher_tags_ffi.BothResult
import uniffi.launcher_tags_ffi.ErrorKind
import uniffi.launcher_tags_ffi.KvaesitsoInfo
import uniffi.launcher_tags_ffi.OneWayResult
import uniffi.launcher_tags_ffi.PackageChange
import uniffi.launcher_tags_ffi.PendingMode
import uniffi.launcher_tags_ffi.QuickSearchInfo
import uniffi.launcher_tags_ffi.SyncException

@OptIn(ExperimentalCoroutinesApi::class)
class AppViewModelTest {
  private val dispatcher = StandardTestDispatcher()
  private lateinit var dir: File

  @Before
  fun setUp() {
    Dispatchers.setMain(dispatcher)
    dir = Files.createTempDirectory("lts").toFile()
  }

  @After
  fun tearDown() {
    Dispatchers.resetMain()
    dir.deleteRecursively()
  }

  private class FakeIo : DocumentIo {
    val written = mutableMapOf<String, ByteArray>()

    override fun read(uri: String) = DocumentIo.Picked(uri.substringAfterLast('/'), uri.toByteArray())

    override fun write(uri: String, bytes: ByteArray) {
      written[uri] = bytes
    }
  }

  private class FakeEngine : SyncEngine {
    var lastPending: PendingMode? = null
    var lastState: ByteArray? = null
    var refuseUntilDiscard = false

    override fun coreVersion() = "test"

    override fun inspectKvaesitso(bytes: ByteArray) = KvaesitsoInfo("1.9", "1.41.0", 1L, 1u, 7u, 4u, 2u, emptyList())

    override fun inspectQuickSearch(bytes: ByteArray): QuickSearchInfo {
      if (String(bytes).contains("bad")) throw SyncException.Failed(ErrorKind.SETTINGS_NOT_EXPORTED, "no SETTINGS")
      return QuickSearchInfo(2L, 3u, 1u, listOf("SETTINGS"))
    }

    override fun oneWay(kvaesitso: ByteArray, quicksearch: ByteArray) =
      OneWayResult(
        "QOUT".toByteArray(),
        "{}",
        listOf(PackageChange("a.b", true, listOf("x"), emptyList())),
        1u,
        0u,
        1u,
        emptyList(),
      )

    override fun both(
      kvaesitso: ByteArray,
      quicksearch: ByteArray,
      state: ByteArray?,
      pending: PendingMode,
      allowMassDelete: Boolean,
      allowStale: Boolean,
    ): BothResult {
      lastPending = pending
      lastState = state
      if (refuseUntilDiscard && pending != PendingMode.DISCARD) {
        throw SyncException.Failed(ErrorKind.PENDING_NOT_IMPORTED, "never imported")
      }
      return BothResult(
        initial = state == null,
        quicksearch = "QOUT".toByteArray(),
        kvaesitso = "KOUT".toByteArray(),
        state = "STATE2".toByteArray(),
        reportJson = "{}",
        toQuicksearch = listOf(PackageChange("a.b", false, listOf("x"), emptyList())),
        toKvaesitso = listOf(PackageChange("c.d", false, listOf("y"), listOf("z"))),
        deletions = 1u,
        conflicts = emptyList(),
        unresolvableApps = emptyList(),
        pendingNote = null,
        warnings = emptyList(),
      )
    }
  }

  private fun TestScope.vm(engine: SyncEngine, io: DocumentIo, store: SyncStore) =
    AppViewModel(engine, io, store, worker = dispatcher, clock = { 1000L }).also { advanceUntilIdle() }

  @Test
  fun oneWayHappyPath() = runTest(dispatcher) {
    val io = FakeIo()
    val store = SyncStore(dir)
    val vm = vm(FakeEngine(), io, store)
    vm.startWizard()
    vm.pickKvaesitso("content://k/backup.kvaesitso")
    advanceUntilIdle()
    assertEquals("backup.kvaesitso", vm.state.value.kvaesitso?.name)
    vm.goTo(Step.ExportQuickSearch)
    vm.pickQuickSearch("content://q/qs.quicksearch")
    advanceUntilIdle()
    vm.goTo(Step.Preview)
    advanceUntilIdle()
    assertTrue(vm.state.value.preview is SyncPreview.OneWay)
    assertTrue(vm.state.value.needsQuickSearchSave)
    assertFalse(vm.state.value.needsKvaesitsoSave)
    vm.goTo(Step.Save)
    vm.saveQuickSearch("content://out/q")
    advanceUntilIdle()
    assertArrayEquals("QOUT".toByteArray(), io.written["content://out/q"])
    assertTrue(vm.state.value.committed)
    assertFalse("one-way never creates sync state", store.hasState())
    assertEquals(1, store.history().size)
    assertFalse(store.history()[0].twoWay)
  }

  @Test
  fun twoWayCommitsStateOnlyAfterBothSaved() = runTest(dispatcher) {
    val io = FakeIo()
    val store = SyncStore(dir)
    val engine = FakeEngine()
    val vm = vm(engine, io, store)
    vm.setTwoWay(true)
    vm.startWizard()
    vm.pickKvaesitso("content://k/k")
    vm.pickQuickSearch("content://q/q")
    advanceUntilIdle()
    vm.goTo(Step.Preview)
    advanceUntilIdle()
    assertNull(engine.lastState)
    vm.saveQuickSearch("content://out/q")
    advanceUntilIdle()
    assertFalse(store.hasState())
    assertFalse(vm.state.value.allSaved)
    vm.saveKvaesitso("content://out/k")
    advanceUntilIdle()
    assertArrayEquals("KOUT".toByteArray(), io.written["content://out/k"])
    assertArrayEquals("STATE2".toByteArray(), store.readState())
    assertTrue(store.history()[0].twoWay)
    assertEquals(1, store.history()[0].deletions)
    assertTrue(vm.state.value.hasSyncState)
  }

  @Test
  fun refusalCanBeOverriddenAndUsesStoredState() = runTest(dispatcher) {
    val store = SyncStore(dir)
    store.commitState("STATE1".toByteArray())
    val engine = FakeEngine().apply { refuseUntilDiscard = true }
    val vm = vm(engine, FakeIo(), store)
    vm.setTwoWay(true)
    vm.startWizard()
    vm.pickKvaesitso("content://k/k")
    vm.pickQuickSearch("content://q/q")
    advanceUntilIdle()
    vm.goTo(Step.Preview)
    advanceUntilIdle()
    assertEquals(ErrorKind.PENDING_NOT_IMPORTED, vm.state.value.error?.kind)
    assertNull(vm.state.value.preview)
    vm.applyOverride { it.copy(pending = PendingMode.DISCARD) }
    advanceUntilIdle()
    assertNull(vm.state.value.error)
    assertEquals(PendingMode.DISCARD, engine.lastPending)
    assertArrayEquals("STATE1".toByteArray(), engine.lastState)
  }

  @Test
  fun invalidInputShowsErrorAndBlocksNext() = runTest(dispatcher) {
    val vm = vm(FakeEngine(), FakeIo(), SyncStore(dir))
    vm.startWizard()
    vm.pickQuickSearch("content://q/bad")
    advanceUntilIdle()
    assertEquals(ErrorKind.SETTINGS_NOT_EXPORTED, vm.state.value.error?.kind)
    assertNull(vm.state.value.quickSearch)
  }

  @Test
  fun cannotGoBackAfterSaving() = runTest(dispatcher) {
    val vm = vm(FakeEngine(), FakeIo(), SyncStore(dir))
    vm.startWizard()
    vm.pickKvaesitso("content://k/k")
    vm.pickQuickSearch("content://q/q")
    advanceUntilIdle()
    vm.goTo(Step.Preview)
    advanceUntilIdle()
    vm.goTo(Step.Save)
    vm.saveQuickSearch("content://out/q")
    advanceUntilIdle()
    vm.back()
    assertEquals(Step.Save, vm.state.value.step)
  }
}
