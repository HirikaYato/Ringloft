use std::path::PathBuf;

use tauri::State;

use crate::error::{RingloftError, Result};
use crate::playlist::{
    PlaylistMeta, PlaylistRow, PlaylistSearch, PlaylistSources, SortKey, SourcesRefresh,
};
use crate::settings::{RepeatMode, Settings};
use crate::state::AppState;

/// Разворачивание папок — это обход диска, поэтому в блокирующий пул.
#[tauri::command]
pub async fn playlist_add(
    state: State<'_, AppState>,
    paths: Vec<String>,
    at: Option<u32>,
) -> Result<u32> {
    let playlist = state.playlist.clone();
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    let at = at.map(|value| value as usize);

    tauri::async_runtime::spawn_blocking(move || playlist.add_paths(&paths, at) as u32)
        .await
        .map_err(|err| RingloftError::Internal(format!("добавление файлов: {err}")))
}

#[tauri::command]
pub fn playlist_meta(state: State<'_, AppState>) -> PlaylistMeta {
    state.playlist.meta()
}

/// Окно строк для виртуализированного списка.
#[tauri::command]
pub fn playlist_rows(state: State<'_, AppState>, offset: u32, limit: u32) -> Vec<PlaylistRow> {
    let mut rows = state
        .playlist
        .rows(offset as usize, (limit as usize).min(2000));
    with_library_marks(&state, &mut rows);
    rows
}

/// Прослушивания и оценка живут в библиотеке, а не в строке плейлиста:
/// подтягиваем их одним запросом на окно строк.
fn with_library_marks(state: &AppState, rows: &mut [PlaylistRow]) {
    let paths: Vec<&str> = rows.iter().map(|row| row.path.as_str()).collect();
    let marks = state.library.track_marks(&paths);
    for row in rows.iter_mut() {
        if let Some((plays, rating)) = marks.get(&row.path) {
            row.play_count = Some(*plays);
            row.rating = Some(*rating);
        }
    }
}

/// Поиск по открытой вкладке. Страницами — как и обычные строки.
#[tauri::command]
pub fn playlist_search(
    state: State<'_, AppState>,
    query: String,
    offset: u32,
    limit: u32,
) -> PlaylistSearch {
    let (total, mut rows) = state
        .playlist
        .search(&query, offset as usize, (limit as usize).min(2000));
    with_library_marks(&state, &mut rows);
    PlaylistSearch {
        total: total as u32,
        rows,
    }
}

/// Убирает из списка строки, файлов которых больше нет.
#[tauri::command]
pub fn playlist_drop_missing(state: State<'_, AppState>) -> u32 {
    state.playlist.drop_missing() as u32
}

/// Перенос выбранных строк в другую вкладку: перетаскиванием на её ярлык.
#[tauri::command]
pub fn playlist_transfer(
    state: State<'_, AppState>,
    indexes: Vec<u32>,
    tab_id: u64,
    copy: Option<bool>,
) -> u32 {
    let indexes: Vec<usize> = indexes.into_iter().map(|index| index as usize).collect();
    state
        .playlist
        .transfer(&indexes, tab_id, copy.unwrap_or(false)) as u32
}

/// Порядок воспроизведения вперёд: при перемешивании его больше негде увидеть.
#[tauri::command]
pub fn playlist_upcoming(state: State<'_, AppState>, limit: u32) -> Vec<PlaylistRow> {
    let settings = state.settings.get();
    state.playlist.upcoming_rows(
        settings.playback.repeat,
        settings.playback.shuffle,
        (limit as usize).min(500),
    )
}

/// Пути выбранных строк — их просит редактор тегов.
#[tauri::command]
pub fn playlist_paths(state: State<'_, AppState>, indexes: Vec<u32>) -> Vec<String> {
    let indexes: Vec<usize> = indexes.into_iter().map(|value| value as usize).collect();
    state
        .playlist
        .paths_at(&indexes)
        .into_iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect()
}

#[tauri::command]
pub fn playlist_remove(state: State<'_, AppState>, ids: Vec<u64>) {
    state.playlist.remove(&ids);
}

#[tauri::command]
pub fn playlist_clear(state: State<'_, AppState>) {
    state.playlist.clear();
}

#[tauri::command]
pub fn playlist_move(state: State<'_, AppState>, ids: Vec<u64>, to_index: u32) {
    state.playlist.move_items(&ids, to_index as usize);
}

#[tauri::command]
pub fn playlist_sort(state: State<'_, AppState>, key: SortKey, ascending: bool) {
    state.playlist.sort_by(key, ascending);
}

/// Импорт списка: M3U или CUE, определяем по расширению.
/// Файловый ввод-вывод плюс чтение тегов, поэтому в пул.
#[tauri::command]
pub async fn playlist_import(state: State<'_, AppState>, path: String) -> Result<u32> {
    let playlist = state.playlist.clone();
    let path = PathBuf::from(path);
    let is_cue = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("cue"));

    tauri::async_runtime::spawn_blocking(move || {
        if is_cue {
            playlist.import_cue(&path)
        } else {
            playlist.import_m3u(&path)
        }
    })
        .await
        .map_err(|err| RingloftError::Internal(format!("импорт списка: {err}")))?
        .map(|(added, listed)| {
            if added < listed {
                tracing::warn!(added, listed, "часть файлов из списка не нашлась");
            }
            added as u32
        })
}

#[tauri::command]
pub async fn playlist_export_m3u(state: State<'_, AppState>, path: String) -> Result<u32> {
    let playlist = state.playlist.clone();
    let path = PathBuf::from(path);
    tauri::async_runtime::spawn_blocking(move || playlist.export_m3u(&path))
        .await
        .map_err(|err| RingloftError::Internal(format!("экспорт списка: {err}")))?
        .map(|count| count as u32)
}

#[tauri::command]
pub fn playlist_create_tab(state: State<'_, AppState>, name: Option<String>) -> u64 {
    state.playlist.create_tab(name)
}

#[tauri::command]
pub fn playlist_rename_tab(state: State<'_, AppState>, id: u64, name: String) {
    state.playlist.rename_tab(id, name);
}

/// Перетащили вкладку: встаёт перед `before`, без него — в конец.
#[tauri::command]
pub fn playlist_move_tab(state: State<'_, AppState>, id: u64, before: Option<u64>) {
    state.playlist.move_tab(id, before);
}

#[tauri::command]
pub fn playlist_close_tab(state: State<'_, AppState>, id: u64) {
    state.playlist.close_tab(id);
}

#[tauri::command]
pub fn playlist_activate_tab(state: State<'_, AppState>, id: u64) {
    state.playlist.activate_tab(id);
}

/// Очередь перебивает обычный порядок: эти строки играют следующими.
#[tauri::command]
pub fn playlist_enqueue(state: State<'_, AppState>, ids: Vec<u64>) {
    state.playlist.enqueue(&ids);
}

#[tauri::command]
pub fn playlist_queue_rows(state: State<'_, AppState>) -> Vec<PlaylistRow> {
    state.playlist.queue_rows()
}

#[tauri::command]
pub fn playlist_queue_clear(state: State<'_, AppState>) {
    state.playlist.clear_queue();
}

#[tauri::command]
pub fn playlist_play_index(state: State<'_, AppState>, index: u32) -> Result<()> {
    state.controls().play_index(index as usize)
}

#[tauri::command]
pub fn player_next(state: State<'_, AppState>) -> Result<()> {
    state.controls().next(true)
}

#[tauri::command]
pub fn player_prev(state: State<'_, AppState>) -> Result<()> {
    state.controls().prev()
}

#[tauri::command]
pub fn player_set_repeat(state: State<'_, AppState>, repeat: RepeatMode) -> Result<Settings> {
    let shuffle = state.settings.get().playback.shuffle;
    state.controls().set_mode(repeat, shuffle)
}

#[tauri::command]
pub fn player_set_shuffle(state: State<'_, AppState>, shuffle: bool) -> Result<Settings> {
    let repeat = state.settings.get().playback.repeat;
    state.controls().set_mode(repeat, shuffle)
}

#[tauri::command]
pub fn playlist_sources(state: State<'_, AppState>, id: u64) -> PlaylistSources {
    state.playlist.sources_of(id)
}

#[tauri::command]
pub fn playlist_set_sources(state: State<'_, AppState>, id: u64, sources: Vec<String>) {
    let dirs = sources.into_iter().map(PathBuf::from).collect();
    state.playlist.set_sources(id, dirs);
}

#[tauri::command]
pub fn playlist_restore_excluded(state: State<'_, AppState>, id: u64) {
    state.playlist.restore_excluded(id);
}

/// Обновить список по его папкам; без `id` — все списки с источниками.
/// Это обход диска, поэтому в блокирующий пул.
#[tauri::command]
pub async fn playlist_refresh_sources(
    state: State<'_, AppState>,
    id: Option<u64>,
) -> Result<SourcesRefresh> {
    let playlist = state.playlist.clone();
    super::blocking(move || {
        Ok(match id {
            Some(id) => playlist.refresh_sources(id),
            None => playlist.refresh_all_sources(),
        })
    })
    .await
}
