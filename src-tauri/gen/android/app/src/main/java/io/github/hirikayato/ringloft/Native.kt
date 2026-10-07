package io.github.hirikayato.ringloft

import android.content.Context

/** Нативные вызовы плеера, которых нет у Tauri (см. src-tauri/src/android.rs). */
object Native {
  init {
    // Tauri грузит ту же библиотеку сам, но позже; повторная загрузка ничего не делает.
    System.loadLibrary("ringloft_lib")
  }

  /** Отдаёт Rust-части JavaVM и контекст приложения: без них cpal не найдёт устройства вывода. */
  @JvmStatic external fun initContext(context: Context)
}
