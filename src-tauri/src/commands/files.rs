//! Операции с файлами на диске: в корзину и переименование по шаблону.
//!
//! Все три команды блокирующие: это диск и разбор тегов.

use std::path::PathBuf;

use tauri::State;

use super::blocking;
use crate::error::Result;
use crate::files::{self, FileOpResult, RenamePreview};
use crate::state::AppState;

/// Сколько строк показываем в предпросмотре: дальше он только мешает.
const PREVIEW_LIMIT: usize = 40;

/// Убирает файлы в корзину и выкидывает их строки из всех вкладок.
#[tauri::command]
pub async fn files_to_trash(
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Result<FileOpResult> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    let playlist = state.playlist.clone();

    blocking(move || {
        let result = files::to_trash(&paths);
        if result.done > 0 {
            playlist.drop_paths(&paths);
        }
        Ok(result)
    })
    .await
}

#[tauri::command]
pub async fn files_rename_preview(
    paths: Vec<String>,
    pattern: String,
) -> Result<Vec<RenamePreview>> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    blocking(move || Ok(files::preview(&paths, &pattern, PREVIEW_LIMIT))).await
}

/// Переименовывает файлы и подменяет пути в строках: иначе они указывали бы
/// в пустоту, и трек перестал бы играть.
#[tauri::command]
pub async fn files_rename(
    state: State<'_, AppState>,
    paths: Vec<String>,
    pattern: String,
) -> Result<FileOpResult> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    let playlist = state.playlist.clone();

    blocking(move || {
        let (result, renamed) = files::rename(&paths, &pattern);
        if !renamed.is_empty() {
            playlist.replace_paths(&renamed);
        }
        Ok(result)
    })
    .await
}
