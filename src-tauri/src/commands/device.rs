//! То, что зависит от устройства: разрешения системы и корни хранилища.
//!
//! Запросы состояния есть на всех платформах — их зовут общие сторы фронта,
//! и на десктопе они отвечают «всё разрешено». Просьба выдать разрешение
//! есть только на телефоне.

use std::path::PathBuf;

use serde::Serialize;
use tauri::AppHandle;
use ts_rs::TS;

use super::blocking;
use crate::error::Result;
use crate::folders::{self, FolderEntry, FolderListing};

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "Permissions.ts")]
pub struct Permissions {
    /// Плеер может ходить по диску обычными путями.
    pub files: bool,
    /// Карточка плеера в шторке и на экране блокировки будет видна.
    pub notifications: bool,
}

// На десктопе просить нечего, и команду с этим аргументом никто не зовёт.
#[cfg_attr(desktop, allow(dead_code))]
#[derive(Debug, Clone, Copy, serde::Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "PermissionKind.ts")]
pub enum PermissionKind {
    Files,
    Notifications,
}

#[tauri::command]
pub async fn permissions_status() -> Result<Permissions> {
    #[cfg(target_os = "android")]
    return blocking(|| {
        Ok(Permissions {
            files: crate::android::files_allowed(),
            notifications: crate::android::notifications_allowed(),
        })
    })
    .await;
    #[cfg(not(target_os = "android"))]
    Ok(Permissions {
        files: true,
        notifications: true,
    })
}

/// Показывает системный вопрос или экран настроек. Ответ пользователя сюда
/// не возвращается: он уходит в систему, а фронт перечитывает состояние,
/// когда окно снова на экране.
#[cfg(mobile)]
#[tauri::command]
pub fn permissions_request(kind: PermissionKind) {
    #[cfg(target_os = "android")]
    crate::android::request_permission(kind);
    #[cfg(not(target_os = "android"))]
    let _ = kind;
}

/// Сворачивает приложение: так кончается «назад» на корневом экране.
#[cfg(mobile)]
#[tauri::command]
pub fn app_to_background() {
    #[cfg(target_os = "android")]
    crate::android::move_to_back();
}

/// Красит значки строки состояния под тему плеера (см. `set_light_bars`).
#[cfg(mobile)]
#[tauri::command]
pub fn system_bars(light: bool) {
    #[cfg(target_os = "android")]
    crate::android::set_light_bars(light);
    #[cfg(not(target_os = "android"))]
    let _ = light;
}

/// С чего начинать выбор папки: память телефона и карты памяти, а на
/// десктопе — домашняя папка.
#[tauri::command]
pub async fn storage_roots(app: AppHandle) -> Result<Vec<FolderEntry>> {
    #[cfg(target_os = "android")]
    {
        let _ = app;
        blocking(|| Ok(crate::android::storage_roots())).await
    }
    #[cfg(not(target_os = "android"))]
    {
        use tauri::Manager;
        let home = app.path().home_dir()?;
        Ok(vec![FolderEntry {
            name: "Домашняя папка".to_owned(),
            path: home.to_string_lossy().into_owned(),
        }])
    }
}

/// Содержимое папки для окна выбора. `extensions` — какие файлы показывать;
/// пусто — только папки.
#[tauri::command]
pub async fn storage_list(path: String, extensions: Vec<String>) -> Result<FolderListing> {
    blocking(move || folders::list(&PathBuf::from(path), &extensions)).await
}
