//! Теги по звуку (AcoustID + MusicBrainz). Записывает найденное обычная
//! правка тегов (`tags_write`) — здесь только поиск.

use std::path::PathBuf;

use tauri::State;

use crate::error::{RingloftError, Result};
use crate::identify::TagCandidate;
use crate::state::AppState;

/// Варианты тегов для одного файла. По одному файлу за вызов: фронт сам
/// ведёт очередь и показывает ход работы, а AcoustID не любит пачки.
#[tauri::command]
pub async fn identify_track(state: State<'_, AppState>, path: String) -> Result<Vec<TagCandidate>> {
    let key = state
        .settings
        .get()
        .acoustid
        .api_key
        .filter(|key| !key.trim().is_empty())
        .ok_or_else(|| {
            RingloftError::Internal("нужен ключ AcoustID — он вводится в настройках, раздел «AcoustID»".into())
        })?;
    let path = PathBuf::from(path);
    super::blocking(move || crate::identify::identify(&path, key.trim())).await
}

/// Жанр альбома из MusicBrainz — по запросу, перед записью тегов.
#[tauri::command]
pub async fn identify_genre(release_group_id: String) -> Result<Option<String>> {
    super::blocking(move || crate::identify::genre(&release_group_id)).await
}
