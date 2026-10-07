//! Обвязка приложения на телефоне — пара к `desktop.rs` с теми же входами.
//!
//! Окна, трея, второй копии и командной строки здесь нет. Системный плеер
//! (сервис, MediaSession, уведомление) подключается тоже отсюда.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::Wry;

use crate::media_controls::MediaAction;
use crate::settings::SettingsStore;

pub fn plugins(builder: tauri::Builder<Wry>) -> tauri::Builder<Wry> {
    builder
}

pub fn setup(_app: &tauri::AppHandle, _settings: &Arc<SettingsStore>) {}

/// Командной строки у приложения на телефоне нет: файл «открыть с помощью»
/// приезжает намерением (intent) уже после запуска.
pub fn launch_paths() -> Vec<PathBuf> {
    Vec::new()
}

pub fn launch_action() -> Option<MediaAction> {
    None
}
