package io.github.hirikayato.ringloft

import android.content.Intent
import android.os.Bundle
import android.view.inputmethod.InputMethodManager
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.core.view.WindowCompat

class MainActivity : TauriActivity() {
  /** Тёмные значки строки состояния — для светлой темы плеера, светлые — для тёмной. */
  fun setLightBars(light: Boolean) {
    val bars = WindowCompat.getInsetsController(window, window.decorView)
    bars.isAppearanceLightStatusBars = light
    bars.isAppearanceLightNavigationBars = light
  }
  /**
   * Клавиатура для поля, которому фокус дали не касанием (поиск, диалог
   * ввода): вебвью на такой фокус её не открывает. Сфокусирован при этом
   * сам вебвью — его и просим.
   */
  fun showKeyboard() {
    val view = currentFocus ?: return
    getSystemService(InputMethodManager::class.java)?.showSoftInput(view, 0)
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    // До super.onCreate: он запускает Rust-часть, а движку звука контекст нужен сразу.
    Native.start(this)
    Permissions.attach(this)
    super.onCreate(savedInstanceState)
    Native.activityCreated()
    // Системная кнопка «назад» уходит интерфейсу: он закрывает верхний слой
    // (лист, «сейчас играет», страницу библиотеки), а с корня сам просит
    // свернуть приложение (src/lib/back.ts). Добавлено после super.onCreate,
    // поэтому срабатывает раньше обработчиков Tauri: те смотрят в историю
    // вебвью, а она про слои интерфейса ничего не знает.
    onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() = Native.backPressed()
    })
  }

  override fun onDestroy() {
    Permissions.detach(this)
    super.onDestroy()
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    // Только отладочная сборка: закрыть окно с компьютера, не трогая процесс, —
    // так проверяется, что музыка и сервис переживают уничтожение Activity.
    //   adb shell am start -n io.github.hirikayato.ringloft/.MainActivity --ez ringloft.finish true
    if (BuildConfig.DEBUG && intent.getBooleanExtra("ringloft.finish", false)) {
      finishAndRemoveTask()
    }
  }
}
