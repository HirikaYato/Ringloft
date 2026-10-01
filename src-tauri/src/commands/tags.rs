use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri::ipc::Response;
use ts_rs::TS;

use super::blocking;
use crate::error::{RingloftError, Result};
use crate::lyrics::{self, Lyrics, LyricsFetch};
use crate::state::AppState;
use crate::tag_edit::{self, TagEdit};
use crate::tags::{self, TrackTags};

/// Итог групповой правки. Один битый файл не должен отменять правку
/// остальных, поэтому считаем и то, и другое.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "TagWriteResult.ts")]
pub struct TagWriteResult {
    pub written: u32,
    pub failed: u32,
    /// Причина первой неудачи — её и показываем пользователю.
    pub message: Option<String>,
}

/// Теги читаем в блокирующем пуле: это файловый ввод-вывод, и он не должен
/// задерживать ни UI-поток, ни асинхронный рантайм Tauri.
#[tauri::command]
pub async fn tags_read(path: String) -> Result<TrackTags> {
    blocking(move || tags::read(&PathBuf::from(path))).await
}

/// Запись тегов идёт в блокирующем пуле: это чтение и перезапись файлов.
/// После неё строки плейлиста перечитывают теги сами.
#[tauri::command]
pub async fn tags_write(
    state: State<'_, AppState>,
    paths: Vec<String>,
    edit: TagEdit,
) -> Result<TagWriteResult> {
    let playlist = state.playlist.clone();
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();

    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut result = TagWriteResult {
            written: 0,
            failed: 0,
            message: None,
        };
        for path in &paths {
            match tag_edit::write(path, &edit) {
                Ok(()) => result.written += 1,
                Err(err) => {
                    tracing::warn!(%err, path = %path.display(), "теги не записались");
                    result.failed += 1;
                    result.message.get_or_insert_with(|| err.to_string());
                }
            }
        }
        playlist.refresh_meta(&paths);
        result
    })
    .await
    .map_err(|err| RingloftError::Internal(format!("запись тегов: {err}")))?;

    Ok(result)
}

/// Текст песни — отдельной командой, чтобы не раздувать теги: его просят
/// только когда панель текста открыта. Сначала смотрим `.lrc` рядом с
/// файлом, потом тег.
#[tauri::command]
pub async fn lyrics_read(path: String) -> Result<Lyrics> {
    blocking(move || Ok(lyrics::read(&PathBuf::from(path)))).await
}

/// Ищет текст в интернете (lrclib.net) и кладёт найденное файлом рядом с
/// треком. В сеть плеер ходит только отсюда — то есть по кнопке.
#[tauri::command]
pub async fn lyrics_fetch(path: String) -> Result<LyricsFetch> {
    blocking(move || lyrics::fetch(&PathBuf::from(path))).await
}

/// Сколько треков берём из библиотеки за раз: больше — это уже не «загрузить
/// тексты», а ночь работы.
const LYRICS_LIBRARY_LIMIT: u32 = 5_000;

/// Пакетная загрузка текстов для выбранных файлов.
#[tauri::command]
pub fn lyrics_scan_start(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> bool {
    let paths = paths.into_iter().map(PathBuf::from).collect();
    state.lyrics.start(app, paths)
}

/// То же для всей открытой вкладки: пути собираем в Rust — фронт держит
/// только окно видимых строк.
#[tauri::command]
pub fn lyrics_scan_playlist(app: AppHandle, state: State<'_, AppState>) -> bool {
    let paths = state.playlist.paths();
    state.lyrics.start(app, paths)
}

/// И для всей библиотеки.
#[tauri::command]
pub async fn lyrics_scan_library(app: AppHandle, state: State<'_, AppState>) -> Result<bool> {
    let library = state.library.clone();
    let paths = blocking(move || library.all_paths(LYRICS_LIBRARY_LIMIT)).await?;
    Ok(state
        .lyrics
        .start(app, paths.into_iter().map(PathBuf::from).collect()))
}

#[tauri::command]
pub fn lyrics_scan_cancel(state: State<'_, AppState>) {
    state.lyrics.cancel();
}

#[tauri::command]
pub fn lyrics_scanning(state: State<'_, AppState>) -> bool {
    state.lyrics.is_running()
}

/// Обложка уходит во фронт сырыми байтами (ArrayBuffer), а не base64:
/// у картинок альбомов это регулярно мегабайты.
#[tauri::command]
pub async fn cover_read(path: String) -> Result<Response> {
    let bytes = blocking(move || tags::read_cover(&PathBuf::from(path))).await?;
    Ok(Response::new(bytes))
}

/// Подсчёт громкости для выбранных файлов. `force` — считать заново даже
/// там, где метка уже есть.
#[tauri::command]
pub fn gain_scan_start(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
    force: Option<bool>,
) -> bool {
    let paths = paths.into_iter().map(PathBuf::from).collect();
    state.gain.start(app, paths, force.unwrap_or(false))
}

/// То же для всей открытой вкладки: пути собираем в Rust — фронт держит
/// только окно видимых строк.
#[tauri::command]
pub fn gain_scan_playlist(
    app: AppHandle,
    state: State<'_, AppState>,
    force: Option<bool>,
) -> bool {
    let paths = state.playlist.paths();
    state.gain.start(app, paths, force.unwrap_or(false))
}

#[tauri::command]
pub fn gain_scan_cancel(state: State<'_, AppState>) {
    state.gain.cancel();
}

#[tauri::command]
pub fn gain_scanning(state: State<'_, AppState>) -> bool {
    state.gain.is_running()
}
