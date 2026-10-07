package io.github.hirikayato.ringloft

import android.content.Intent
import android.os.SystemClock
import android.util.Log

/**
 * Что сейчас играет — так, как это видит система. Пишет сюда Rust (через
 * [Native]), читает [PlaybackService]. Всё — на главном потоке.
 */
object NowPlaying {
  // Те же числа отдаёт Rust (src-tauri/src/android/media.rs).
  const val IDLE = 0
  const val PLAYING = 1
  const val PAUSED = 2

  var title = ""
    private set
  var artist: String? = null
    private set
  var album: String? = null
    private set
  /** Отрицательная — длительности нет (радио). */
  var durationMs = -1L
    private set
  var coverPath: String? = null
    private set
  var status = IDLE
    private set
  var positionMs = 0L
    private set
  /** Когда позиция была верна, по часам `elapsedRealtime`: дальше система считает сама. */
  var positionAt = 0L
    private set

  fun setTrack(title: String, artist: String?, album: String?, durationMs: Long) {
    this.title = title
    this.artist = artist
    this.album = album
    this.durationMs = durationMs
    // Обложка нового трека приедет следом; чужую до тех пор не показываем.
    coverPath = null
    changed(track = true)
  }

  fun setCover(path: String?) {
    coverPath = path
    changed(track = true)
  }

  fun setPlayback(status: Int, positionMs: Long) {
    this.status = status
    this.positionMs = positionMs
    positionAt = SystemClock.elapsedRealtime()
    changed(track = false)
  }

  /** `track` — сменилось то, что играет, а не только позиция и статус. */
  private fun changed(track: Boolean) {
    val service = PlaybackService.instance
    if (service != null) {
      service.render(track)
    } else if (status == PLAYING) {
      // Сервис поднимается с началом воспроизведения, а не с запуском
      // приложения: пока ничего не играет, держать его незачем.
      try {
        val context = Native.appContext
        context.startForegroundService(Intent(context, PlaybackService::class.java))
      } catch (err: Exception) {
        // Из фона система сервис запустить не даст (Android 12+).
        Log.w("Ringloft", "сервис воспроизведения не запустился", err)
      }
    }
  }
}
