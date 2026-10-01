//! Профиль наушников (AutoEQ): поиск по базе, загрузка, свой файл.

use std::path::PathBuf;

use tauri::{AppHandle, Manager, State};

use crate::audio::AudioCmd;
use crate::error::{RingloftError, Result};
use crate::headphones::HeadphoneHit;
use crate::settings::{HeadphoneProfile, Settings};
use crate::state::AppState;

#[tauri::command]
pub async fn headphones_search(app: AppHandle, query: String) -> Result<Vec<HeadphoneHit>> {
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|err| RingloftError::Internal(format!("нет папки кэша: {err}")))?;
    super::blocking(move || crate::headphones::search(&cache, &query)).await
}

/// Скачать профиль выбранной модели и сразу включить его.
#[tauri::command]
pub async fn headphones_apply(state: State<'_, AppState>, hit: HeadphoneHit) -> Result<Settings> {
    let profile = super::blocking(move || crate::headphones::download(&hit)).await?;
    install(&state, profile)
}

/// Свой `ParametricEQ.txt` — например, сделанный на autoeq.app.
#[tauri::command]
pub async fn headphones_import(state: State<'_, AppState>, path: String) -> Result<Settings> {
    let path = PathBuf::from(path);
    let profile = super::blocking(move || crate::headphones::import(&path)).await?;
    install(&state, profile)
}

/// Включить или выключить профиль; выключенный остаётся в настройках.
#[tauri::command]
pub fn headphones_set_enabled(state: State<'_, AppState>, enabled: bool) -> Result<Settings> {
    let settings = state
        .settings
        .patch(serde_json::json!({ "audio": { "headphoneEnabled": enabled } }))?;
    let profile = settings.audio.headphone.clone().filter(|_| enabled);
    state.engine.send(AudioCmd::SetHeadphone(profile))?;
    Ok(settings)
}

fn install(state: &AppState, profile: HeadphoneProfile) -> Result<Settings> {
    let settings = state.settings.patch(serde_json::json!({
        "audio": { "headphone": profile, "headphoneEnabled": true }
    }))?;
    state
        .engine
        .send(AudioCmd::SetHeadphone(settings.audio.headphone.clone()))?;
    tracing::info!(name = profile.name, source = profile.source, "профиль наушников включён");
    Ok(settings)
}
