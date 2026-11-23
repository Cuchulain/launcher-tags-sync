package cz.jancejka.launchertagssync.data

import android.content.ContentResolver
import android.net.Uri
import android.provider.OpenableColumns
import java.io.IOException

/**
 * Reading/writing user-picked documents (Storage Access Framework). Documents are addressed
 * by their URI string so that the view model stays free of Android classes (unit-testable).
 */
interface DocumentIo {
  fun read(uri: String): Picked

  fun write(uri: String, bytes: ByteArray)

  data class Picked(val name: String, val bytes: ByteArray)
}

class ContentResolverIo(private val resolver: ContentResolver) : DocumentIo {
  override fun read(uri: String): DocumentIo.Picked = read(Uri.parse(uri))

  override fun write(uri: String, bytes: ByteArray) = write(Uri.parse(uri), bytes)

  private fun read(uri: Uri): DocumentIo.Picked {
    val name =
      resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
        if (c.moveToFirst()) c.getString(0) else null
      } ?: uri.lastPathSegment ?: "?"
    val bytes =
      resolver.openInputStream(uri)?.use { input ->
        val out = java.io.ByteArrayOutputStream()
        val buf = ByteArray(64 * 1024)
        var total = 0L
        while (true) {
          val n = input.read(buf)
          if (n < 0) break
          total += n
          if (total > MAX_BYTES) throw IOException("file is larger than ${MAX_BYTES / (1024 * 1024)} MiB")
          out.write(buf, 0, n)
        }
        out.toByteArray()
      } ?: throw IOException("cannot open the selected file")
    return DocumentIo.Picked(name, bytes)
  }

  private fun write(uri: Uri, bytes: ByteArray) {
    // "wt" truncates an existing document; verify by reading it back.
    resolver.openOutputStream(uri, "wt")?.use { it.write(bytes) } ?: throw IOException("cannot open the target file")
    val back = resolver.openInputStream(uri)?.use { it.readBytes() }
    if (back == null || !back.contentEquals(bytes)) throw IOException("verification of the saved file failed")
  }

  companion object {
    const val MAX_BYTES = 256L * 1024 * 1024
  }
}
