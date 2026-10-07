package io.github.hirikayato.ringloft

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import androidx.core.content.ContextCompat

/**
 * Вежливость к остальному звуку на телефоне: аудиофокус (звонок, навигатор,
 * другой плеер) и выдернутые наушники. Решений о звуке сам не принимает —
 * только просит плеер о паузе и продолжении теми же действиями, что и кнопки.
 */
class AudioFocus(private val context: Context) {
  private val manager = context.getSystemService(AudioManager::class.java)
  private var playing = false
  private var held = false
  private var noisyRegistered = false

  /**
   * Пауза не пользователя, а наша — из-за чужого короткого звука. Когда он
   * кончится, продолжаем сами; до тех пор фокус не отдаём и сервис не гасим.
   */
  var waiting = false
    private set

  private val request = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
    .setAudioAttributes(
      AudioAttributes.Builder()
        .setUsage(AudioAttributes.USAGE_MEDIA)
        .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
        .build()
    )
    .setOnAudioFocusChangeListener { change ->
      when (change) {
        // Фокус забрали насовсем (включили другой плеер): сами не вернёмся.
        AudioManager.AUDIOFOCUS_LOSS -> {
          held = false
          waiting = false
          Native.mediaAction(Native.PAUSE, 0)
        }
        // Забрали ненадолго (звонок, голосовая подсказка).
        AudioManager.AUDIOFOCUS_LOSS_TRANSIENT -> {
          held = false
          if (playing) {
            waiting = true
            Native.mediaAction(Native.PAUSE, 0)
          }
        }
        AudioManager.AUDIOFOCUS_GAIN -> {
          held = true
          if (waiting) {
            waiting = false
            Native.mediaAction(Native.PLAY, 0)
          }
        }
        // «Можно приглушить» не обрабатываем: с Android 8 система приглушает сама.
      }
    }
    .build()

  // Наушники выдернули или колонка отвалилась: звук сейчас пойдёт из динамика.
  private val noisy = object : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
      Native.mediaAction(Native.PAUSE, 0)
    }
  }

  /** Зовётся на каждое изменение состояния плеера. */
  fun update(playing: Boolean) {
    this.playing = playing
    if (playing) {
      // Заиграло — значит, ждать больше нечего (пользователь продолжил сам).
      waiting = false
      if (!held) {
        held = manager.requestAudioFocus(request) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED
        // Фокус не дали (идёт звонок) — играть поверх нельзя.
        if (!held) Native.mediaAction(Native.PAUSE, 0)
      }
      if (!noisyRegistered) {
        ContextCompat.registerReceiver(
          context, noisy, IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY),
          ContextCompat.RECEIVER_NOT_EXPORTED
        )
        noisyRegistered = true
      }
    } else {
      if (noisyRegistered) {
        context.unregisterReceiver(noisy)
        noisyRegistered = false
      }
      if (!waiting) abandon()
    }
  }

  fun release() {
    waiting = false
    update(false)
  }

  private fun abandon() {
    manager.abandonAudioFocusRequest(request)
    held = false
  }
}
