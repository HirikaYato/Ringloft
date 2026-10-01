use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::{RingloftError, Result};
use crate::lastfm::LastfmStatus;
use crate::state::AppState;

#[tauri::command]
pub fn lastfm_status(state: State<'_, AppState>) -> LastfmStatus {
    state.scrobbler.status(&state.settings)
}

/// Первый шаг входа: берём одноразовый токен и открываем страницу
/// подтверждения. Сеть — в блокирующий пул.
#[tauri::command]
pub async fn lastfm_begin_auth(app: AppHandle, state: State<'_, AppState>) -> Result<String> {
    let scrobbler = state.scrobbler.clone();
    let settings = state.settings.clone();

    let url = tauri::async_runtime::spawn_blocking(move || scrobbler.begin_auth(&settings))
        .await
        .map_err(|err| RingloftError::Internal(format!("Last.fm: {err}")))??;

    // Браузер открываем сами: фронту для этого понадобилось бы отдельное право.
    if let Err(err) = app.opener().open_url(&url, None::<&str>) {
        tracing::warn!(%err, "страница Last.fm не открылась");
    }
    Ok(url)
}

/// Второй шаг: пользователь подтвердил доступ в браузере.
#[tauri::command]
pub async fn lastfm_finish_auth(state: State<'_, AppState>) -> Result<LastfmStatus> {
    let scrobbler = state.scrobbler.clone();
    let settings = state.settings.clone();

    tauri::async_runtime::spawn_blocking(move || scrobbler.finish_auth(&settings))
        .await
        .map_err(|err| RingloftError::Internal(format!("Last.fm: {err}")))?
}

#[tauri::command]
pub fn lastfm_disconnect(state: State<'_, AppState>) -> Result<LastfmStatus> {
    state.scrobbler.disconnect(&state.settings)
}
