package io.github.hirikayato.ringloft

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.drawable.Icon
import android.media.session.MediaSession

/** Карточка плеера в шторке и на экране блокировки — уведомление [PlaybackService]. */
class PlayerNotification(private val service: Service, private val session: MediaSession) {
  companion object {
    const val ID = 1
    /** Ключ, под которым кнопка уведомления несёт код действия из [Native]. */
    const val EXTRA_ACTION = "action"
    private const val CHANNEL = "playback"
  }

  init {
    service.getSystemService(NotificationManager::class.java).createNotificationChannel(
      // LOW — без звука и всплывания: карточка плеера не должна мешать музыке.
      NotificationChannel(
        CHANNEL, service.getString(R.string.playback_channel), NotificationManager.IMPORTANCE_LOW
      )
    )
  }

  fun build(playing: Boolean, cover: Bitmap?): Notification {
    val toggle = if (playing) {
      action(android.R.drawable.ic_media_pause, R.string.action_pause, Native.PAUSE)
    } else {
      action(android.R.drawable.ic_media_play, R.string.action_play, Native.PLAY)
    }
    return Notification.Builder(service, CHANNEL)
      .setSmallIcon(R.drawable.ic_notification)
      .setContentTitle(NowPlaying.title)
      .setContentText(NowPlaying.artist)
      .setSubText(NowPlaying.album)
      .setLargeIcon(cover)
      .setContentIntent(openApp())
      .setVisibility(Notification.VISIBILITY_PUBLIC)
      .setOngoing(playing)
      .setShowWhen(false)
      // Кнопки отсюда берёт Android 8–12; с 13-го система рисует их по
      // состоянию сессии, а эти не показывает.
      .addAction(action(android.R.drawable.ic_media_previous, R.string.action_previous, Native.PREVIOUS))
      .addAction(toggle)
      .addAction(action(android.R.drawable.ic_media_next, R.string.action_next, Native.NEXT))
      .setStyle(
        Notification.MediaStyle()
          .setMediaSession(session.sessionToken)
          .setShowActionsInCompactView(0, 1, 2)
      )
      .build()
  }

  /** Нажатие на карточку открывает плеер. */
  fun openApp(): PendingIntent {
    val intent = Intent(service, MainActivity::class.java)
    return PendingIntent.getActivity(service, 0, intent, PendingIntent.FLAG_IMMUTABLE)
  }

  private fun action(icon: Int, label: Int, code: Int): Notification.Action {
    val intent = Intent(service, PlaybackService::class.java).putExtra(EXTRA_ACTION, code)
    // Код действия — он же номер запроса: иначе система склеит намерения в одно.
    val pending = PendingIntent.getService(
      service, code, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
    )
    return Notification.Action.Builder(
      Icon.createWithResource(service, icon), service.getString(label), pending
    ).build()
  }
}
