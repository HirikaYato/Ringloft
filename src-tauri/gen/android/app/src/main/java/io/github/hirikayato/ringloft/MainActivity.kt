package io.github.hirikayato.ringloft

import android.os.Bundle
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    // До super.onCreate: он запускает Rust-часть, а движку звука контекст нужен сразу.
    Native.initContext(applicationContext)
    super.onCreate(savedInstanceState)
  }
}
