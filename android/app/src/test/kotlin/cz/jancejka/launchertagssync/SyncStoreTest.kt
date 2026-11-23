package cz.jancejka.launchertagssync

import cz.jancejka.launchertagssync.data.HistoryEntry
import cz.jancejka.launchertagssync.data.SyncStore
import java.io.File
import java.nio.file.Files
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class SyncStoreTest {
  private val dir: File = Files.createTempDirectory("lts-store").toFile()

  @After fun cleanup() = dir.deleteRecursively().let {}

  @Test
  fun stateCommitKeepsPreviousAndResetRemoves() {
    val s = SyncStore(dir)
    assertNull(s.readState())
    s.commitState("one".toByteArray())
    s.commitState("two".toByteArray())
    assertArrayEquals("two".toByteArray(), s.readState())
    assertArrayEquals("one".toByteArray(), File(dir, "sync-state.json.prev").readBytes())
    s.resetState()
    assertFalse(s.hasState())
    assertArrayEquals("two".toByteArray(), File(dir, "sync-state.json.prev").readBytes())
    assertTrue(dir.listFiles()!!.none { it.name.endsWith(".tmp") })
  }

  @Test
  fun historyIsNewestFirstBoundedAndSurvivesCorruption() {
    val s = SyncStore(dir)
    repeat(5) { s.addHistory(HistoryEntry(it.toLong(), it % 2 == 0, false, it, 0, 0, 0, 0), keep = 3) }
    assertEquals(listOf(4L, 3L, 2L), s.history().map { it.timeMs })
    File(dir, "history.json").writeText("{broken")
    assertEquals(emptyList<HistoryEntry>(), s.history())
  }
}
