use std::path::PathBuf;

use tauri::State;

use super::blocking;
use crate::error::{RingloftError, Result};
use crate::library::{
    AlbumRow, DuplicateGroup, LibraryStats, LibraryTrack, NamedCount, SmartRules, TrackFilter,
};
use crate::settings::Settings;
use crate::state::AppState;

/// Запускает сканирование папок из настроек. `false` — скан уже идёт.
#[tauri::command]
pub fn library_scan_start(state: State<'_, AppState>) -> bool {
    let folders = state.settings.get().library.folders;
    state.library.start_scan(folders)
}

#[tauri::command]
pub fn library_scan_cancel(state: State<'_, AppState>) {
    state.library.cancel_scan();
}

#[tauri::command]
pub fn library_scanning(state: State<'_, AppState>) -> bool {
    state.library.is_scanning()
}

/// Чтения из базы уводим в блокирующий пул: при активном сканировании
/// читающее соединение может подождать до пяти секунд.
#[tauri::command]
pub async fn library_stats(state: State<'_, AppState>) -> Result<LibraryStats> {
    let library = state.library.clone();
    blocking(move || library.stats()).await
}

/// Итоги прослушивания за период.
#[tauri::command]
pub async fn library_listening(
    state: State<'_, AppState>,
    period: crate::library::StatsPeriod,
) -> Result<crate::library::ListeningStats> {
    let library = state.library.clone();
    let now = crate::lastfm::now_unix();
    blocking(move || library.listening_stats(period, now)).await
}

/// Сколько групп дубликатов и пропавших файлов отдаём за раз: дальше список
/// всё равно не разобрать руками.
const DUPLICATE_LIMIT: usize = 200;
const MISSING_LIMIT: usize = 1000;

/// Похожие треки: совпали исполнитель с названием (а без тегов — имя файла).
#[tauri::command]
pub async fn library_duplicates(state: State<'_, AppState>) -> Result<Vec<DuplicateGroup>> {
    let library = state.library.clone();
    blocking(move || library.duplicates(DUPLICATE_LIMIT)).await
}

/// Треки, которых больше нет на диске. Опрос файловой системы — поэтому в пул.
#[tauri::command]
pub async fn library_missing(state: State<'_, AppState>) -> Result<Vec<LibraryTrack>> {
    let library = state.library.clone();
    blocking(move || library.missing(MISSING_LIMIT)).await
}

/// Убирает строки из библиотеки. Файлы не трогает: их либо уже нет, либо с
/// ними отдельно разбираются через корзину.
#[tauri::command]
pub async fn library_forget(state: State<'_, AppState>, paths: Vec<String>) -> Result<u32> {
    let library = state.library.clone();
    blocking(move || library.forget(&paths)).await
}

#[tauri::command]
pub async fn library_search(
    state: State<'_, AppState>,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<LibraryTrack>> {
    let library = state.library.clone();
    blocking(move || library.search(&query, limit.unwrap_or(100))).await
}

#[tauri::command]
pub async fn library_artists(state: State<'_, AppState>) -> Result<Vec<NamedCount>> {
    let library = state.library.clone();
    blocking(move || library.artists()).await
}

#[tauri::command]
pub async fn library_genres(state: State<'_, AppState>) -> Result<Vec<NamedCount>> {
    let library = state.library.clone();
    blocking(move || library.genres()).await
}

#[tauri::command]
pub async fn library_folders(state: State<'_, AppState>) -> Result<Vec<NamedCount>> {
    let library = state.library.clone();
    blocking(move || library.folders()).await
}

#[tauri::command]
pub async fn library_albums(
    state: State<'_, AppState>,
    artist: Option<String>,
) -> Result<Vec<AlbumRow>> {
    let library = state.library.clone();
    blocking(move || library.albums(artist.as_deref())).await
}

/// Треки выбранного узла дерева. `scope` — album | artist | genre | folder.
#[tauri::command]
pub async fn library_tracks(
    state: State<'_, AppState>,
    scope: String,
    value: String,
) -> Result<Vec<LibraryTrack>> {
    let filter = match scope.as_str() {
        "album" => TrackFilter::Album(value),
        "artist" => TrackFilter::Artist(value),
        "genre" => TrackFilter::Genre(value),
        "folder" => TrackFilter::Folder(value),
        other => {
            return Err(RingloftError::Internal(format!(
                "неизвестный раздел библиотеки: {other}"
            )));
        }
    };

    let library = state.library.clone();
    blocking(move || library.tracks_of(&filter)).await
}

/// Путь к миниатюре обложки. Вебвью открывает его по asset-протоколу.
#[tauri::command]
pub async fn library_album_cover(
    state: State<'_, AppState>,
    album_key: String,
) -> Result<Option<String>> {
    let library = state.library.clone();
    blocking(move || library.album_cover(&album_key)).await
}

/// Добавляет папки в библиотеку и сразу их сканирует.
#[tauri::command]
pub fn library_folders_add(state: State<'_, AppState>, paths: Vec<String>) -> Result<Settings> {
    let mut folders = state.settings.get().library.folders;
    for path in paths {
        let path = PathBuf::from(path);
        if path.is_dir() && !folders.contains(&path) {
            folders.push(path);
        }
    }

    let settings = state
        .settings
        .patch(serde_json::json!({ "library": { "folders": folders } }))?;
    state.library.start_scan(settings.library.folders.clone());
    if settings.library.watch {
        state.library.watch(&settings.library.folders);
    }
    Ok(settings)
}

#[tauri::command]
pub fn library_folder_remove(state: State<'_, AppState>, path: String) -> Result<Settings> {
    let removed = PathBuf::from(path);
    let folders: Vec<PathBuf> = state
        .settings
        .get()
        .library
        .folders
        .into_iter()
        .filter(|folder| folder != &removed)
        .collect();

    // Треки удалённой папки должны уйти из базы сразу: обычная чистка при
    // скане их не тронет, она ограничена оставшимися папками.
    let forgotten = state.library.forget_folder(&removed)?;
    tracing::info!(forgotten, path = %removed.display(), "папка убрана из библиотеки");

    let settings = state
        .settings
        .patch(serde_json::json!({ "library": { "folders": folders } }))?;
    state.library.watch(&settings.library.folders);
    Ok(settings)
}


/// Треки умного списка. Условия приходят с фронта, но в SQL попадают только
/// через параметры — разбор в `library/smart.rs`.
#[tauri::command]
pub async fn library_smart_tracks(
    state: State<'_, AppState>,
    rules: SmartRules,
) -> Result<Vec<LibraryTrack>> {
    let library = state.library.clone();
    blocking(move || library.smart_tracks(&rules)).await
}

/// Оценка трека: 0 — снять, иначе 1..5.
#[tauri::command]
pub async fn library_set_rating(
    state: State<'_, AppState>,
    path: String,
    rating: u32,
) -> Result<bool> {
    let library = state.library.clone();
    blocking(move || library.set_rating(std::path::Path::new(&path), rating)).await
}

/// Найти обложку альбома в интернете и вписать её в его файлы.
#[tauri::command]
pub async fn covers_fetch_album(
    state: State<'_, AppState>,
    album_key: String,
) -> Result<crate::cover_fetch::CoverFetchResult> {
    let library = state.library.clone();
    blocking(move || library.fetch_album_cover(&album_key)).await
}

/// То же для играющего трека (правый клик по обложке в нижней панели).
/// Трек из библиотеки — весь его альбом; вне её — только сам файл.
#[tauri::command]
pub async fn covers_fetch_track(
    state: State<'_, AppState>,
    path: String,
) -> Result<crate::cover_fetch::CoverFetchResult> {
    let library = state.library.clone();
    blocking(move || {
        if let Some(key) = library.album_key_of(&path)? {
            return library.fetch_album_cover(&key);
        }
        let tags = crate::tags::read(std::path::Path::new(&path))?;
        let artist = tags.album_artist.or(tags.artist).unwrap_or_default();
        let album = tags.album.unwrap_or_default();
        Ok(match crate::cover_fetch::find(&artist, &album)? {
            Some(found) => crate::cover_fetch::embed(&[path], &found),
            None => crate::cover_fetch::CoverFetchResult::default(),
        })
    })
    .await
}

/// Все альбомы библиотеки без обложки — фоновой задачей.
#[tauri::command]
pub fn covers_scan_start(app: tauri::AppHandle, state: State<'_, AppState>) -> bool {
    state.covers.start(app, state.library.clone())
}

#[tauri::command]
pub fn covers_scan_cancel(state: State<'_, AppState>) {
    state.covers.cancel();
}

#[tauri::command]
pub fn covers_scanning(state: State<'_, AppState>) -> bool {
    state.covers.is_running()
}
