//! Операции с самими файлами: в корзину и переименование по шаблону.
//!
//! Две вещи здесь сознательно сделаны «трусливо»: удаление идёт **только в
//! корзину** (мимо неё восстановить нечего), а переименование никогда не
//! перезаписывает существующий файл — такой случай считается неудачей.

use std::path::{Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

use crate::tags::{self, TrackTags};

/// Запрещённые в именах файлов символы (набор Windows, он строже).
const FORBIDDEN: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
/// Предел длины имени без расширения: у файловых систем лимит в байтах,
/// а кириллица в UTF-8 занимает по два.
const MAX_STEM: usize = 120;

#[derive(Debug, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "FileOpResult.ts")]
pub struct FileOpResult {
    pub done: u32,
    pub failed: u32,
    /// Причина первой неудачи: показываем её, а не «что-то пошло не так».
    pub message: Option<String>,
}

impl FileOpResult {
    fn fail(&mut self, reason: String) {
        self.failed += 1;
        if self.message.is_none() {
            self.message = Some(reason);
        }
    }
}

/// Строка предпросмотра: что во что превратится и почему не превратится.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "RenamePreview.ts")]
pub struct RenamePreview {
    pub from: String,
    pub to: String,
    pub problem: Option<String>,
}

/// Убирает файлы в корзину. Мимо корзины не удаляем вообще никогда.
pub fn to_trash(paths: &[PathBuf]) -> FileOpResult {
    let mut result = FileOpResult::default();
    for path in paths {
        match trash::delete(path) {
            Ok(()) => result.done += 1,
            Err(err) => {
                tracing::warn!(path = %path.display(), %err, "в корзину не убралось");
                result.fail(format!("{}: {err}", name_of(path)));
            }
        }
    }
    result
}

/// Что получится из шаблона. Отдельно от самого переименования: диалог
/// показывает это до того, как что-то произойдёт.
pub fn preview(paths: &[PathBuf], pattern: &str, limit: usize) -> Vec<RenamePreview> {
    paths
        .iter()
        .take(limit)
        .map(|path| match target(path, pattern) {
            Ok(target) => RenamePreview {
                from: name_of(path),
                to: name_of(&target),
                problem: problem_with(path, &target),
            },
            Err(reason) => RenamePreview {
                from: name_of(path),
                to: String::new(),
                problem: Some(reason),
            },
        })
        .collect()
}

/// Переименовывает файлы. Возвращает итог и пары «было — стало»: по ним
/// плейлист правит свои строки, иначе они указывали бы в пустоту.
pub fn rename(paths: &[PathBuf], pattern: &str) -> (FileOpResult, Vec<(PathBuf, PathBuf)>) {
    let mut result = FileOpResult::default();
    let mut renamed = Vec::new();

    for path in paths {
        let target = match target(path, pattern) {
            Ok(target) => target,
            Err(reason) => {
                result.fail(format!("{}: {reason}", name_of(path)));
                continue;
            }
        };

        if target == *path {
            continue;
        }
        if let Some(reason) = problem_with(path, &target) {
            result.fail(format!("{}: {reason}", name_of(path)));
            continue;
        }

        match std::fs::rename(path, &target) {
            Ok(()) => {
                move_sidecar(path, &target);
                result.done += 1;
                renamed.push((path.clone(), target));
            }
            Err(err) => {
                tracing::warn!(path = %path.display(), %err, "переименовать не удалось");
                result.fail(format!("{}: {err}", name_of(path)));
            }
        }
    }

    (result, renamed)
}

/// Текст рядом с треком должен уехать вместе с ним, иначе синхронный текст
/// просто потеряется.
fn move_sidecar(from: &Path, to: &Path) {
    let lrc = from.with_extension("lrc");
    if lrc.is_file() {
        let _ = std::fs::rename(lrc, to.with_extension("lrc"));
    }
}

fn problem_with(path: &Path, target: &Path) -> Option<String> {
    if target == path {
        return Some("уже так называется".to_owned());
    }
    if target.exists() {
        return Some("файл с таким именем уже есть".to_owned());
    }
    None
}

/// Куда переименуем. Ошибка — это «нечем»: из шаблона вышло пустое имя.
fn target(path: &Path, pattern: &str) -> Result<PathBuf, String> {
    let tags = tags::read(path).unwrap_or_else(|_| TrackTags::default());
    let stem = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("");
    let name = fill(pattern, &tags, stem);
    if name.is_empty() {
        return Err("по шаблону вышло пустое имя".to_owned());
    }

    let mut target = path.to_path_buf();
    match path.extension().and_then(|ext| ext.to_str()) {
        Some(extension) => target.set_file_name(format!("{name}.{extension}")),
        None => target.set_file_name(name),
    }
    Ok(target)
}

/// Подстановка тегов в шаблон. Незнакомые фигурные скобки остаются как есть —
/// опечатку в шаблоне видно в предпросмотре.
fn fill(pattern: &str, tags: &TrackTags, stem: &str) -> String {
    let track = tags.track.map(|no| format!("{no:02}")).unwrap_or_default();
    let year = tags.year.map(|year| year.to_string()).unwrap_or_default();
    let filled = pattern
        .replace("{artist}", tags.artist.as_deref().unwrap_or_default())
        .replace("{title}", tags.title.as_deref().unwrap_or_default())
        .replace("{album}", tags.album.as_deref().unwrap_or_default())
        .replace("{albumartist}", tags.album_artist.as_deref().unwrap_or_default())
        .replace("{genre}", tags.genre.as_deref().unwrap_or_default())
        .replace("{track}", &track)
        .replace("{year}", &year)
        .replace("{filename}", stem);

    clamp(&sanitize(&tidy(&filled)))
}

/// Пустой тег оставляет за собой разделители: «{artist} - {title}» без
/// исполнителя дал бы «- Название».
fn tidy(name: &str) -> String {
    let single_spaces = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut cleaned = single_spaces;
    while cleaned.contains(" - - ") {
        cleaned = cleaned.replace(" - - ", " - ");
    }
    cleaned
        .trim_matches(|c: char| c == '-' || c == '_' || c == '.' || c.is_whitespace())
        .to_owned()
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if FORBIDDEN.contains(&c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect()
}

/// Обрезка по границе символа: длинное имя не должно ломать запись.
fn clamp(name: &str) -> String {
    if name.len() <= MAX_STEM {
        return name.to_owned();
    }
    let mut end = MAX_STEM;
    while end > 0 && !name.is_char_boundary(end) {
        end -= 1;
    }
    name[..end].trim_end().to_owned()
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags() -> TrackTags {
        TrackTags {
            title: Some("Панелька".to_owned()),
            artist: Some("Хаски".to_owned()),
            album: Some("Любимые песни".to_owned()),
            track: Some(3),
            year: Some(2017),
            ..TrackTags::default()
        }
    }

    #[test]
    fn fills_the_pattern() {
        let filled = fill("{track} - {artist} - {title}", &tags(), "stem");
        assert_eq!(filled, "03 - Хаски - Панелька");
        assert_eq!(fill("{year} {album}", &tags(), "stem"), "2017 Любимые песни");
        assert_eq!(fill("{filename}", &tags(), "как было"), "как было");
    }

    /// Пустой тег не должен оставлять за собой разделители.
    #[test]
    fn empty_tags_do_not_leave_dashes() {
        let empty = TrackTags::default();
        assert_eq!(fill("{artist} - {title}", &empty, "stem"), "");
        let only_title = TrackTags {
            title: Some("Один".to_owned()),
            ..TrackTags::default()
        };
        assert_eq!(fill("{artist} - {title}", &only_title, "stem"), "Один");
        assert_eq!(fill("{track} - {artist} - {title}", &only_title, "s"), "Один");
    }

    #[test]
    fn forbidden_characters_are_replaced() {
        let nasty = TrackTags {
            title: Some("AC/DC: \"Live\"?".to_owned()),
            ..TrackTags::default()
        };
        assert_eq!(fill("{title}", &nasty, "s"), "AC_DC_ _Live__");
    }

    /// Незнакомый плейсхолдер остаётся в имени: опечатку видно в предпросмотре.
    #[test]
    fn unknown_placeholder_stays_visible() {
        assert_eq!(fill("{artst} - {title}", &tags(), "s"), "{artst} - Панелька");
    }

    #[test]
    fn long_names_are_cut_on_a_character_boundary() {
        let long = TrackTags {
            title: Some("Я".repeat(200)),
            ..TrackTags::default()
        };
        let filled = fill("{title}", &long, "s");
        assert!(filled.len() <= MAX_STEM, "{}", filled.len());
        assert!(filled.chars().all(|c| c == 'Я'));
    }

    #[test]
    fn renames_the_file_and_its_lyrics() {
        let dir = std::env::temp_dir().join(format!("ringloft-rename-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("01. трек.wav");
        crate::audio::test_wav::sine(&path, 44_100, 2, 4_410, 0.5);
        std::fs::write(path.with_extension("lrc"), "[00:01.00]раз").expect("текст");
        let edit = crate::tag_edit::TagEdit {
            title: Some("Панелька".to_owned()),
            artist: Some("Хаски".to_owned()),
            ..Default::default()
        };
        crate::tag_edit::write(&path, &edit).expect("теги");

        let preview = preview(std::slice::from_ref(&path), "{artist} - {title}", 10);
        assert_eq!(preview[0].to, "Хаски - Панелька.wav");
        assert_eq!(preview[0].problem, None);

        let (result, renamed) = rename(std::slice::from_ref(&path), "{artist} - {title}");
        assert_eq!((result.done, result.failed), (1, 0));
        assert_eq!(renamed.len(), 1);

        let target = dir.join("Хаски - Панелька.wav");
        assert!(target.is_file());
        assert!(!path.exists());
        assert!(target.with_extension("lrc").is_file(), "текст уехал вместе с треком");

        // Повторный прогон по тому же шаблону ничего не делает.
        let (again, pairs) = rename(std::slice::from_ref(&target), "{artist} - {title}");
        assert_eq!((again.done, again.failed, pairs.len()), (0, 0, 0));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Занятое имя — это неудача, а не перезапись.
    #[test]
    fn existing_target_is_not_overwritten() {
        let dir = std::env::temp_dir().join(format!("ringloft-clash-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let first = dir.join("a.wav");
        let second = dir.join("b.wav");
        for path in [&first, &second] {
            crate::audio::test_wav::sine(path, 44_100, 2, 4_410, 0.5);
            let edit = crate::tag_edit::TagEdit {
                title: Some("Одно и то же".to_owned()),
                ..Default::default()
            };
            crate::tag_edit::write(path, &edit).expect("теги");
        }

        let (result, renamed) = rename(&[first.clone(), second.clone()], "{title}");
        assert_eq!(result.done, 1);
        assert_eq!(result.failed, 1);
        assert!(
            result.message.as_deref().unwrap_or_default().contains("уже есть"),
            "{:?}",
            result.message
        );
        assert_eq!(renamed.len(), 1);
        assert!(dir.join("Одно и то же.wav").is_file());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_is_a_failure_not_a_panic() {
        let result = to_trash(&[PathBuf::from("/такого/файла/нет.flac")]);
        assert_eq!((result.done, result.failed), (0, 1));
        assert!(result.message.is_some());
    }
}
