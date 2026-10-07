package io.github.hirikayato.ringloft

import android.content.Intent
import android.os.Bundle
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    // До super.onCreate: он запускает Rust-часть, а движку звука контекст нужен сразу.
    Native.start(this)
    Permissions.attach(this)
    super.onCreate(savedInstanceState)
    Native.activityCreated()
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
