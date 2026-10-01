use tauri::State;

use crate::error::Result;
use crate::settings::Settings;
use crate::state::AppState;

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> Settings {
    state.settings.get()
}

/// Частичное обновление: фронт шлёт только изменившиеся поля на любой глубине,
/// бэкенд мержит, нормализует и возвращает актуальный слепок целиком.
#[tauri::command]
pub fn settings_patch(state: State<'_, AppState>, patch: serde_json::Value) -> Result<Settings> {
    state.settings.patch(patch)
}

/// Семейства шрифтов с кириллицей, установленные в системе, — для выбора
/// шрифта в настройках. Вебвью перечислять шрифты не умеет, поэтому
/// спрашиваем fontconfig. Нет его (Windows) — список пустой, и остаются
/// встроенные шрифты.
#[tauri::command]
pub async fn fonts_installed() -> Result<Vec<String>> {
    super::blocking(|| Ok(installed_fonts())).await
}

fn installed_fonts() -> Vec<String> {
    let output = std::process::Command::new("fc-list")
        .args([":lang=ru", "--format", "%{family[0]}\\n"])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    let mut families: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect();
    families.sort_by_key(|name| name.to_lowercase());
    families.dedup();
    families
}
