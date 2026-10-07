//! Содержимое папки для своего окна выбора файлов.
//!
//! Нужно оно телефону: системный выбор файлов на Android отдаёт не пути, а
//! адреса `content://`, которыми плеер не пользуется — с «Доступом ко всем
//! файлам» он ходит по диску сам, как на десктопе. На десктопе выбирают
//! системным диалогом, но команда есть и там: окно выбора так проще проверять.

use std::path::Path;

use serde::Serialize;
use ts_rs::TS;

use crate::error::{Result, RingloftError};

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "FolderEntry.ts")]
pub struct FolderEntry {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "FolderListing.ts")]
pub struct FolderListing {
    pub path: String,
    /// `None` — выше некуда.
    pub parent: Option<String>,
    pub dirs: Vec<FolderEntry>,
    /// Только файлы с запрошенными расширениями; если их не просили — пусто.
    pub files: Vec<FolderEntry>,
}

/// Что лежит в папке. Скрытое (с точки) не показываем: на телефоне это
/// служебные папки приложений, и музыки там не бывает.
pub fn list(path: &Path, extensions: &[String]) -> Result<FolderListing> {
    let entries = std::fs::read_dir(path).map_err(|err| RingloftError::io(path, err))?;

    let mut dirs = Vec::new();
    let mut files = Vec::new();
    // Одна нечитаемая запись — не повод не показать остальные.
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let full = entry.path();
        let item = FolderEntry {
            name,
            path: full.to_string_lossy().into_owned(),
        };
        // `is_dir` идёт по ссылкам: папка, подключённая ссылкой, — тоже папка.
        if full.is_dir() {
            dirs.push(item);
        } else if has_extension(&full, extensions) {
            files.push(item);
        }
    }
    // Сортировка здесь, а не во фронте: порядок один для всех, кто спросит.
    dirs.sort_by_key(|item| item.name.to_lowercase());
    files.sort_by_key(|item| item.name.to_lowercase());

    Ok(FolderListing {
        path: path.to_string_lossy().into_owned(),
        parent: path.parent().map(|parent| parent.to_string_lossy().into_owned()),
        dirs,
        files,
    })
}

fn has_extension(path: &Path, extensions: &[String]) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extensions
                .iter()
                .any(|wanted| wanted.eq_ignore_ascii_case(extension))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ringloft-folders-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Музыка")).unwrap();
        std::fs::create_dir_all(dir.join("album")).unwrap();
        std::fs::create_dir_all(dir.join(".thumbnails")).unwrap();
        for file in ["b.FLAC", "a.mp3", "cover.jpg", ".hidden.mp3"] {
            std::fs::write(dir.join(file), b"").unwrap();
        }
        dir
    }

    fn names(items: &[FolderEntry]) -> Vec<&str> {
        items.iter().map(|item| item.name.as_str()).collect()
    }

    #[test]
    fn lists_folders_and_wanted_files() {
        let dir = fixture("list");
        let listing = list(&dir, &["mp3".into(), "flac".into()]).unwrap();

        // Скрытое не показываем, расширение сверяем без регистра.
        assert_eq!(names(&listing.dirs), ["album", "Музыка"]);
        assert_eq!(names(&listing.files), ["a.mp3", "b.FLAC"]);
        assert_eq!(listing.parent.as_deref(), dir.parent().and_then(|p| p.to_str()));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn without_extensions_shows_only_folders() {
        let dir = fixture("dirs");
        let listing = list(&dir, &[]).unwrap();

        assert_eq!(names(&listing.dirs), ["album", "Музыка"]);
        assert!(listing.files.is_empty());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_folder_is_an_error() {
        assert!(list(Path::new("/такой/папки/нет"), &[]).is_err());
    }
}
