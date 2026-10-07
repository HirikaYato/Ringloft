package io.github.hirikayato.ringloft

import android.app.Notification
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.media.MediaMetadata
import android.media.session.MediaSession
import android.media.session.PlaybackState
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.PowerManager
import android.util.Log

/**
 * Сервис переднего плана: пока он жив, система не трогает процесс, и музыка
 * играет при погашенном экране и без окна. Он же держит MediaSession —
 * карточку плеера в шторке, на экране блокировки и кнопки гарнитуры.
 *
 * Живёт, пока есть что играть: поднимается с началом воспроизведения
 * ([NowPlaying]) и гасит себя сам, когда плеер долго стоит.
 */
class PlaybackService : Service() {
  companion object {
    var instance: PlaybackService? = null
      private set

    /** Сколько сервис ждёт на паузе: продолжить с экрана блокировки можно, пока он жив. */
    private const val PAUSED_LINGER_MS = 10 * 60 * 1000L
    /**
     * После остановки ждём недолго, но не ноль: между треками разной частоты
     * плеер на долю секунды «остановлен», а заново из фона сервис не поднять.
     */
    private const val STOPPED_LINGER_MS = 30 * 1000L
    /** Блокировка сна берётся с запасом и продлевается, пока идут обновления позиции. */
    private const val WAKE_LOCK_MS = 10 * 60 * 1000L
  }

  private lateinit var session: MediaSession
  private lateinit var card: PlayerNotification
  private lateinit var focus: AudioFocus
  private lateinit var wakeLock: PowerManager.WakeLock
  private val handler = Handler(Looper.getMainLooper())
  private val shutdown = Runnable {
    stopForeground(STOP_FOREGROUND_REMOVE)
    stopSelf()
  }

  private var cover: Bitmap? = null
  private var coverOf: String? = null
  private var shownPlaying: Boolean? = null

  override fun onBind(intent: Intent?): IBinder? = null

  override fun onCreate() {
    super.onCreate()
    instance = this
    session = MediaSession(this, "Ringloft").apply { setCallback(callback) }
    card = PlayerNotification(this, session)
    session.setSessionActivity(card.openApp())
    focus = AudioFocus(this)
    wakeLock = getSystemService(PowerManager::class.java)
      .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "Ringloft:playback")
      .apply { setReferenceCounted(false) }
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    // Кнопки уведомления (до Android 13 система берёт их отсюда, а не из сессии).
    intent?.getIntExtra(PlayerNotification.EXTRA_ACTION, -1)?.takeIf { it >= 0 }?.let { Native.mediaAction(it, 0) }
    render(track = true)
    return START_NOT_STICKY
  }

  override fun onDestroy() {
    instance = null
    handler.removeCallbacks(shutdown)
    focus.release()
    if (wakeLock.isHeld) wakeLock.release()
    session.release()
    super.onDestroy()
  }

  private val callback = object : MediaSession.Callback() {
    override fun onPlay() = Native.mediaAction(Native.PLAY, 0)
    override fun onPause() = Native.mediaAction(Native.PAUSE, 0)
    override fun onStop() = Native.mediaAction(Native.STOP, 0)
    override fun onSkipToNext() = Native.mediaAction(Native.NEXT, 0)
    override fun onSkipToPrevious() = Native.mediaAction(Native.PREVIOUS, 0)
    override fun onFastForward() = Native.mediaAction(Native.FORWARD, 0)
    override fun onRewind() = Native.mediaAction(Native.REWIND, 0)
    override fun onSeekTo(pos: Long) = Native.mediaAction(Native.SEEK_TO, pos)
  }

  /**
   * Приводит сессию, уведомление и всё остальное к тому, что лежит в
   * [NowPlaying]. `track` — сменился сам трек или обложка; без него это
   * очередное обновление позиции, и картинки заново не собираются.
   */
  fun render(track: Boolean) {
    val playing = NowPlaying.status == NowPlaying.PLAYING

    if (track) session.setMetadata(metadata())
    session.setPlaybackState(playbackState(playing))
    session.isActive = true

    focus.update(playing)
    // Звук и так не даёт телефону уснуть, но только пока он идёт: стоит
    // декодеру запнуться, и процессор уснёт посреди трека.
    if (playing) wakeLock.acquire(WAKE_LOCK_MS) else if (wakeLock.isHeld) wakeLock.release()

    handler.removeCallbacks(shutdown)
    if (!playing && !focus.waiting) {
      val linger = if (NowPlaying.status == NowPlaying.PAUSED) PAUSED_LINGER_MS else STOPPED_LINGER_MS
      handler.postDelayed(shutdown, linger)
    }

    if (track || shownPlaying != playing) {
      shownPlaying = playing
      show(card.build(playing, coverBitmap()))
    }
  }

  private fun metadata(): MediaMetadata =
    MediaMetadata.Builder()
      .putString(MediaMetadata.METADATA_KEY_TITLE, NowPlaying.title)
      .putString(MediaMetadata.METADATA_KEY_ARTIST, NowPlaying.artist)
      .putString(MediaMetadata.METADATA_KEY_ALBUM, NowPlaying.album)
      .putLong(MediaMetadata.METADATA_KEY_DURATION, NowPlaying.durationMs)
      .putBitmap(MediaMetadata.METADATA_KEY_ALBUM_ART, coverBitmap())
      .build()

  private fun playbackState(playing: Boolean): PlaybackState {
    var actions = PlaybackState.ACTION_PLAY or PlaybackState.ACTION_PAUSE or
      PlaybackState.ACTION_PLAY_PAUSE or PlaybackState.ACTION_STOP or
      PlaybackState.ACTION_SKIP_TO_NEXT or PlaybackState.ACTION_SKIP_TO_PREVIOUS
    // У радио мотать нечего.
    if (NowPlaying.durationMs > 0) {
      actions = actions or PlaybackState.ACTION_SEEK_TO or
        PlaybackState.ACTION_FAST_FORWARD or PlaybackState.ACTION_REWIND
    }
    val state = when (NowPlaying.status) {
      NowPlaying.PLAYING -> PlaybackState.STATE_PLAYING
      NowPlaying.PAUSED -> PlaybackState.STATE_PAUSED
      else -> PlaybackState.STATE_STOPPED
    }
    return PlaybackState.Builder()
      .setActions(actions)
      // Скорость 1 при воспроизведении: дальше позицию система считает сама.
      .setState(state, NowPlaying.positionMs, if (playing) 1f else 0f, NowPlaying.positionAt)
      .build()
  }

  /**
   * Уведомление всегда остаётся уведомлением сервиса переднего плана, в том
   * числе на паузе: так кнопка «играть» работает и из фона (поднять сервис
   * заново оттуда система не даст), а после смерти процесса не остаётся
   * карточки с мёртвыми кнопками.
   */
  private fun show(notification: Notification) {
    try {
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
        startForeground(PlayerNotification.ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK)
      } else {
        startForeground(PlayerNotification.ID, notification)
      }
    } catch (err: Exception) {
      Log.w("Ringloft", "сервис не стал сервисом переднего плана", err)
    }
  }

  /** Обложка читается с диска один раз на трек, а не на каждое обновление. */
  private fun coverBitmap(): Bitmap? {
    val path = NowPlaying.coverPath
    if (path != coverOf) {
      coverOf = path
      cover = path?.let { BitmapFactory.decodeFile(it) }
    }
    return cover
  }
}
