use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::{AppHandle, Manager};
use ts_rs::TS;

use crate::error::Result;

/// Первый вызов `app_info` приходит из onMount вебвью — это честный момент
/// «интерфейс ожил», по нему и меряем холодный старт.
static STARTUP_LOGGED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "AppInfo.ts")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub tauri_version: String,
    pub debug: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "AppPaths.ts")]
pub struct AppPaths {
    pub config: String,
    pub data: String,
    pub cache: String,
    pub log: String,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    if !STARTUP_LOGGED.swap(true, Ordering::Relaxed) {
        tracing::info!(
            elapsed_ms = crate::started_at().elapsed().as_millis() as u64,
            "интерфейс готов"
        );
    }

    AppInfo {
        name: "Ringloft".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        tauri_version: tauri::VERSION.to_owned(),
        debug: cfg!(debug_assertions),
    }
}

/// Пути, которыми пользуется бэкенд. Нужны в разделе «о программе»
/// и чтобы кнопкой открыть папку с логами.
#[tauri::command]
pub fn app_paths(app: AppHandle) -> Result<AppPaths> {
    let path = app.path();
    Ok(AppPaths {
        config: path.app_config_dir()?.display().to_string(),
        data: path.app_data_dir()?.display().to_string(),
        cache: path.app_cache_dir()?.display().to_string(),
        log: path.app_log_dir()?.display().to_string(),
    })
}

/// Проверка подключения к Discord. Ошибка здесь — обычное дело: Discord может
/// быть не запущен, и об этом надо сказать прямо.
#[tauri::command]
pub async fn discord_probe() -> Result<()> {
    super::blocking(crate::discord::probe).await
}

/// Просит рабочий стол показать своё меню окна (то самое, что у KDE
/// появляется по правому клику на заголовке).
///
/// `false` — не получилось, и фронт покажет своё меню. На Wayland просьбу
/// выполняет композитор, и ему нужен серийный номер последнего ввода;
/// синтетическое событие он может и не принять.
/// Меню окна. Сначала просим рабочий стол — у него оно полное (рабочие
/// столы, «поверх остальных», перенос на другой экран). Вернули `false` —
/// фронт покажет своё.
#[tauri::command]
pub async fn window_show_menu(window: tauri::Window) -> bool {
    #[cfg(target_os = "linux")]
    if crate::kwin::is_wayland() {
        // На Wayland синтетическое событие композитор игнорирует, зато у KDE
        // есть свой ярлык на это меню.
        let _ = window.set_focus();
        return tauri::async_runtime::spawn_blocking(crate::kwin::show_window_menu)
            .await
            .unwrap_or(false);
    }
    show_system_menu(&window)
}

/// «Поверх остальных окон». На Wayland приложение не может сделать это само,
/// поэтому там просим KWin.
#[tauri::command]
pub async fn window_always_on_top(
    app: tauri::AppHandle,
    window: tauri::Window,
    on: bool,
) -> Result<bool> {
    let _ = window.set_always_on_top(on);

    #[cfg(target_os = "linux")]
    if crate::kwin::is_wayland() {
        let script = app.path().app_cache_dir()?.join("kwin-keep-above.js");
        if let Some(dir) = script.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        return Ok(
            tauri::async_runtime::spawn_blocking(move || crate::kwin::set_keep_above(on, &script))
                .await
                .unwrap_or(false),
        );
    }

    #[cfg(not(target_os = "linux"))]
    let _ = app;
    Ok(true)
}

#[cfg(target_os = "linux")]
fn show_system_menu(window: &tauri::Window) -> bool {
    use gtk::prelude::*;

    let Ok(gtk_window) = window.gtk_window() else {
        return false;
    };
    let Some(gdk_window) = gtk_window.window() else {
        return false;
    };

    let mut event = gtk::gdk::Event::new(gtk::gdk::EventType::ButtonPress);
    let pointer = gtk_window
        .display()
        .default_seat()
        .and_then(|seat| seat.pointer());
    event.set_device(pointer.as_ref());
    gdk_window.show_window_menu(&mut event)
}

#[cfg(not(target_os = "linux"))]
fn show_system_menu(_window: &tauri::Window) -> bool {
    false
}
