package io.github.hirikayato.ringloft

import android.Manifest
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.Settings
import java.lang.ref.WeakReference

/**
 * Разрешения системы и корни хранилища. Проверить разрешение можно когда
 * угодно, а попросить — только с экрана: вопрос показывает Activity.
 * Ответ пользователя никуда не передаём — фронт перечитывает состояние сам,
 * когда окно снова на экране.
 */
object Permissions {
  private const val REQUEST_FILES = 1
  private const val REQUEST_NOTIFICATIONS = 2
  private const val ASKED_NOTIFICATIONS = "asked-notifications"

  private var activity: WeakReference<MainActivity>? = null

  /** Окно, которое сейчас на экране; `null` — приложение играет без окна. */
  val current: MainActivity?
    get() = activity?.get()

  fun attach(activity: MainActivity) {
    this.activity = WeakReference(activity)
  }

  fun detach(activity: MainActivity) {
    if (this.activity?.get() === activity) this.activity = null
  }

  /**
   * Плеер ходит по диску обычными путями. С Android 11 для этого нужен
   * «Доступ ко всем файлам», до него хватает обычного разрешения на память.
   */
  fun files(context: Context): Boolean =
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
      Environment.isExternalStorageManager()
    } else {
      granted(context, Manifest.permission.WRITE_EXTERNAL_STORAGE)
    }

  /** До Android 13 уведомления разрешены без вопроса. */
  fun notifications(context: Context): Boolean =
    Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
      granted(context, Manifest.permission.POST_NOTIFICATIONS)

  fun requestFiles() {
    val activity = activity?.get() ?: return
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
      activity.requestPermissions(
        arrayOf(Manifest.permission.READ_EXTERNAL_STORAGE, Manifest.permission.WRITE_EXTERNAL_STORAGE),
        REQUEST_FILES
      )
      return
    }
    // Это не вопрос, а экран настроек с переключателем — сразу для нашего
    // приложения. Где такого экрана нет, открываем общий список.
    val own = Intent(
      Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, Uri.parse("package:${activity.packageName}")
    )
    try {
      activity.startActivity(own)
    } catch (_: ActivityNotFoundException) {
      activity.startActivity(Intent(Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION))
    }
  }

  fun requestNotifications() {
    val activity = activity?.get() ?: return
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
    val permission = Manifest.permission.POST_NOTIFICATIONS
    val prefs = activity.getPreferences(Context.MODE_PRIVATE)
    // Свой вопрос система показывает не больше двух раз. Дальше просьба
    // молча ничего не делает — тогда ведём в настройки уведомлений.
    if (prefs.getBoolean(ASKED_NOTIFICATIONS, false) &&
      !activity.shouldShowRequestPermissionRationale(permission)
    ) {
      activity.startActivity(
        Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
          .putExtra(Settings.EXTRA_APP_PACKAGE, activity.packageName)
      )
      return
    }
    prefs.edit().putBoolean(ASKED_NOTIFICATIONS, true).apply()
    activity.requestPermissions(arrayOf(permission), REQUEST_NOTIFICATIONS)
  }

  /** Встроенная память — первой, за ней карты памяти. */
  fun storageRoots(context: Context): Array<String> {
    val roots = LinkedHashSet<String>()
    roots.add(Environment.getExternalStorageDirectory().absolutePath)
    // У каждого тома есть личная папка приложения; корень тома — всё, что до /Android/.
    for (dir in context.getExternalFilesDirs(null)) {
      val path = dir?.absolutePath ?: continue
      val cut = path.indexOf("/Android/")
      if (cut > 0) roots.add(path.substring(0, cut))
    }
    return roots.toTypedArray()
  }

  private fun granted(context: Context, permission: String): Boolean =
    context.checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED
}
