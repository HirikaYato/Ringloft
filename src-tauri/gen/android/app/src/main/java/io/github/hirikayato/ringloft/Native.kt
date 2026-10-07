package io.github.hirikayato.ringloft

import android.content.Context
import android.os.Handler
import android.os.Looper
import androidx.annotation.Keep

/**
 * Мост между Kotlin и Rust-частью плеера (src-tauri/src/android).
 *
 * Работает без Activity: ни одна из сторон не держится за неё, поэтому
 * музыка, уведомление и кнопки живут и тогда, когда окно уничтожено.
 */
// Методы отсюда зовутся из Rust по имени — сжатие кода в релизе их трогать не должно.
@Keep
object Native {
  init {
    // Tauri грузит ту же библиотеку сам, но позже; повторная загрузка ничего не делает.
    System.loadLibrary("ringloft_lib")
  }

  // Действия для mediaAction — те же числа, что в src-tauri/src/android/media.rs.
  const val PLAY = 0
  const val PAUSE = 1
  const val TOGGLE = 2
  const val STOP = 3
  const val NEXT = 4
  const val PREVIOUS = 5
  const val SEEK_TO = 6
  const val FORWARD = 7
  const val REWIND = 8

  private val main = Handler(Looper.getMainLooper())

  /** Контекст приложения, а не Activity: он жив, пока жив процесс. */
  lateinit var appContext: Context
    private set

  /** Зовётся до запуска Rust-части: движку звука контекст нужен сразу. */
  fun start(context: Context) {
    appContext = context.applicationContext
    initContext(appContext)
  }

  /** Отдаёт Rust-части JavaVM и контекст приложения: без них cpal не найдёт устройства вывода. */
  @JvmStatic private external fun initContext(context: Context)

  /** Система создала Activity: если Rust-часть уже работает без окна, оно создаётся заново. */
  @JvmStatic external fun activityCreated()

  /** Нажатие в системном плеере. `value` нужен только перемотке — это позиция в миллисекундах. */
  @JvmStatic external fun mediaAction(action: Int, value: Long)

  // Дальше — вызовы из Rust. Они приходят с его потока, а сервис и
  // MediaSession трогают только с главного.

  @JvmStatic
  fun onTrack(title: String, artist: String?, album: String?, durationMs: Long) {
    main.post { NowPlaying.setTrack(title, artist, album, durationMs) }
  }

  @JvmStatic
  fun onCover(path: String?) {
    main.post { NowPlaying.setCover(path) }
  }

  @JvmStatic
  fun onPlayback(status: Int, positionMs: Long) {
    main.post { NowPlaying.setPlayback(status, positionMs) }
  }

  @JvmStatic fun filesAllowed(): Boolean = Permissions.files(appContext)

  @JvmStatic fun notificationsAllowed(): Boolean = Permissions.notifications(appContext)

  @JvmStatic
  fun requestFiles() {
    main.post { Permissions.requestFiles() }
  }

  @JvmStatic
  fun requestNotifications() {
    main.post { Permissions.requestNotifications() }
  }

  @JvmStatic fun storageRoots(): Array<String> = Permissions.storageRoots(appContext)
}
