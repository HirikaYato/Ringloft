//! Запись тегов. Чтение живёт в `tags.rs`: у него другая цена ошибки —
//! здесь мы меняем файлы пользователя, а не просто показываем строку.

use std::path::Path;

use lofty::config::WriteOptions;
use lofty::file::TaggedFileExt;
use lofty::prelude::{Accessor, ItemKey};
use lofty::tag::items::Timestamp;
use lofty::tag::{Tag, TagExt};
use serde::Deserialize;
use ts_rs::TS;

use crate::error::{RingloftError, Result};

/// Что записать в теги.
///
/// `None` — поле не трогаем (в групповой правке так выглядит «у файлов
/// разные значения, оставить как есть»). Пустая строка и ноль — **удалить**
/// тег: иначе не было бы способа стереть чужой мусор из поля.
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export, export_to = "TagEdit.ts")]
pub struct TagEdit {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub comment: Option<String>,
    pub year: Option<u32>,
    pub track: Option<u32>,
    pub track_total: Option<u32>,
    pub disk: Option<u32>,
}

impl TagEdit {
    /// Пустая правка — повод не трогать файл вообще.
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.artist.is_none()
            && self.album.is_none()
            && self.album_artist.is_none()
            && self.genre.is_none()
            && self.comment.is_none()
            && self.year.is_none()
            && self.track.is_none()
            && self.track_total.is_none()
            && self.disk.is_none()
    }
}

/// Записывает теги в файл. Тег создаётся, если его в файле ещё не было.
pub fn write(path: &Path, edit: &TagEdit) -> Result<()> {
    if edit.is_empty() {
        return Ok(());
    }

    let mut file = lofty::read_from_path(path).map_err(|err| map_error(path, err))?;
    if file.primary_tag_mut().is_none() {
        let kind = file.file_type().primary_tag_type();
        file.insert_tag(Tag::new(kind));
    }
    let Some(tag) = file.primary_tag_mut() else {
        return Err(RingloftError::Tags {
            path: path.display().to_string(),
            message: "в этот формат нельзя записать теги".to_owned(),
        });
    };

    apply(tag, edit);
    tag.save_to_path(path, WriteOptions::default())
        .map_err(|err| map_error(path, err))
}

fn apply(tag: &mut Tag, edit: &TagEdit) {
    if let Some(value) = &edit.title {
        match clean(value) {
            Some(text) => tag.set_title(text),
            None => tag.remove_title(),
        }
    }
    if let Some(value) = &edit.artist {
        match clean(value) {
            Some(text) => tag.set_artist(text),
            None => tag.remove_artist(),
        }
    }
    if let Some(value) = &edit.album {
        match clean(value) {
            Some(text) => tag.set_album(text),
            None => tag.remove_album(),
        }
    }
    if let Some(value) = &edit.genre {
        match clean(value) {
            Some(text) => tag.set_genre(text),
            None => tag.remove_genre(),
        }
    }
    if let Some(value) = &edit.comment {
        match clean(value) {
            Some(text) => tag.set_comment(text),
            None => tag.remove_comment(),
        }
    }
    // У исполнителя альбома своего сеттера нет — он идёт обычным текстовым полем.
    if let Some(value) = &edit.album_artist {
        match clean(value) {
            Some(text) => {
                tag.insert_text(ItemKey::AlbumArtist, text);
            }
            None => tag.remove_key(ItemKey::AlbumArtist),
        }
    }

    if let Some(year) = edit.year {
        match u16::try_from(year) {
            Ok(value) if value > 0 => tag.set_date(Timestamp {
                year: value,
                ..Timestamp::default()
            }),
            _ => tag.remove_date(),
        }
    }
    if let Some(track) = edit.track {
        if track == 0 {
            tag.remove_track();
        } else {
            tag.set_track(track);
        }
    }
    if let Some(total) = edit.track_total {
        if total == 0 {
            tag.remove_track_total();
        } else {
            tag.set_track_total(total);
        }
    }
    if let Some(disk) = edit.disk {
        if disk == 0 {
            tag.remove_disk();
        } else {
            tag.set_disk(disk);
        }
    }
}

/// Пустая строка — не «записать пустоту», а «удалить поле».
fn clean(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn map_error(path: &Path, err: impl std::fmt::Display) -> RingloftError {
    RingloftError::Tags {
        path: path.display().to_string(),
        message: err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::test_wav;

    fn edit() -> TagEdit {
        TagEdit::default()
    }

    /// Пустая правка не должна ни трогать файл, ни падать.
    #[test]
    fn empty_edit_changes_nothing() {
        let path = test_wav::temp_path("tagedit-empty.wav");
        test_wav::constant(&path, 44_100, 2, 1_000, 0.0);
        assert!(write(&path, &edit()).is_ok());
        assert!(crate::tags::read(&path).is_ok());
        let _ = std::fs::remove_file(&path);
    }

    /// Поля записываются, а пустая строка и ноль — стирают тег: иначе нельзя
    /// было бы убрать чужой мусор из поля.
    #[test]
    fn writes_fields_and_clears_them() {
        let path = test_wav::temp_path("tagedit-fields.wav");
        test_wav::constant(&path, 44_100, 2, 1_000, 0.0);

        let full = TagEdit {
            title: Some("  Ночь  ".to_owned()),
            artist: Some("Кишлак".to_owned()),
            album: Some("Сборник".to_owned()),
            album_artist: Some("Разные".to_owned()),
            genre: Some("рэп".to_owned()),
            comment: Some("заметка".to_owned()),
            year: Some(2024),
            track: Some(3),
            track_total: Some(12),
            disk: Some(1),
        };
        write(&path, &full).expect("теги не записались");

        let tags = crate::tags::read(&path).expect("теги не прочитались");
        assert_eq!(tags.title.as_deref(), Some("Ночь"), "пробелы обрезаются");
        assert_eq!(tags.artist.as_deref(), Some("Кишлак"));
        assert_eq!(tags.album.as_deref(), Some("Сборник"));
        assert_eq!(tags.album_artist.as_deref(), Some("Разные"));
        assert_eq!(tags.genre.as_deref(), Some("рэп"));
        assert_eq!(tags.comment.as_deref(), Some("заметка"));
        assert_eq!(tags.year, Some(2024));
        assert_eq!(tags.track, Some(3));
        assert_eq!(tags.track_total, Some(12));
        assert_eq!(tags.disk, Some(1));

        // Чего не указали — не трогаем: альбом должен остаться на месте.
        let partial = TagEdit {
            title: Some(String::new()),
            year: Some(0),
            track: Some(0),
            ..edit()
        };
        write(&path, &partial).expect("правка не записалась");

        let tags = crate::tags::read(&path).expect("теги не прочитались");
        assert_eq!(tags.title, None, "пустая строка стирает поле");
        assert_eq!(tags.year, None, "ноль стирает год");
        assert_eq!(tags.track, None);
        assert_eq!(tags.album.as_deref(), Some("Сборник"), "album не трогали");
        let _ = std::fs::remove_file(&path);
    }
}
