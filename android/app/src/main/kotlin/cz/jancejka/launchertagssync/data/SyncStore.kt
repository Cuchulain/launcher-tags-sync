package cz.jancejka.launchertagssync.data

import java.io.File
import java.io.FileOutputStream
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

/** One finished run. Counts only: no tag texts, no package names. */
@Serializable
data class HistoryEntry(
  val timeMs: Long,
  val twoWay: Boolean,
  val initial: Boolean,
  val quickSearchEntriesChanged: Int,
  val kvaesitsoPackagesChanged: Int,
  val aliasesAdded: Int,
  val deletions: Int,
  val conflicts: Int,
)

/**
 * App-private persistence (never backed up, see manifest): the two-way sync state and the
 * run history. Writes go through a temp file + rename so a crash never leaves half a file.
 */
class SyncStore(private val dir: File) {
  private val stateFile = File(dir, "sync-state.json")
  private val prevStateFile = File(dir, "sync-state.json.prev")
  private val historyFile = File(dir, "history.json")
  private val json = Json { ignoreUnknownKeys = true }

  fun readState(): ByteArray? = stateFile.takeIf { it.isFile }?.readBytes()

  fun hasState(): Boolean = stateFile.isFile

  /** Persist a new state; the previous one is kept as `.prev`. */
  fun commitState(bytes: ByteArray) {
    readState()?.let { writeAtomic(prevStateFile, it) }
    writeAtomic(stateFile, bytes)
  }

  /** Forget the two-way sync state (the next two-way run starts again with a union). */
  fun resetState() {
    readState()?.let { writeAtomic(prevStateFile, it) }
    stateFile.delete()
  }

  fun history(): List<HistoryEntry> =
    runCatching { json.decodeFromString<List<HistoryEntry>>(historyFile.readText()) }.getOrDefault(emptyList())

  fun addHistory(entry: HistoryEntry, keep: Int = 100) {
    val all = (listOf(entry) + history()).take(keep)
    writeAtomic(historyFile, json.encodeToString(all).toByteArray())
  }

  private fun writeAtomic(target: File, bytes: ByteArray) {
    dir.mkdirs()
    val tmp = File(dir, ".${target.name}.tmp")
    FileOutputStream(tmp).use {
      it.write(bytes)
      it.fd.sync()
    }
    if (!tmp.renameTo(target)) {
      tmp.delete()
      throw java.io.IOException("cannot write ${target.name}")
    }
  }
}
