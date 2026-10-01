//! Строки библиотеки: что кладём в базу и что отдаём во фронт.

use std::path::Path;

use lofty::config::ParseOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::prelude::{Accessor, ItemKey};
use lofty::probe::Probe;
use serde::Serialize;
use ts_rs::TS;

/// Трек, готовый к вставке в базу.
#[derive(Debug, Clone)]
pub struct ScannedTrack {
    pub path: String,
    pub folder: String,
    pub title: String,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub album_key: Option<String>,
    pub genre: Option<String>,
    pub year: Option<u32>,
    pub track_no: Option<u32>,
    pub disc_no: Option<u32>,
    pub duration_ms: Option<u64>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub bitrate_kbps: Option<u32>,
    pub size_bytes: u64,
    pub mtime: i64,
}

impl ScannedTrack {
    /// Читает теги. Обложки намеренно не трогаем: на десятках тысяч файлов
    /// это заметно дороже, а картинки всё равно понадобятся только для
    /// галереи — их достанет кэш миниатюр.
    pub fn read(path: &Path, size_bytes: u64, mtime: i64) -> Option<Self> {
        let file = Probe::open(path)
            .ok()?
            .options(ParseOptions::new().read_cover_art(false))
            .read()
            .ok()?;

        let properties = file.properties();
        let tag = file.primary_tag().or_else(|| file.first_tag());

        let title = tag
            .and_then(|tag| tag.title())
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("без названия")
                    .to_owned()
            });

        let artist = tag.and_then(|tag| tag.artist()).map(text);
        let album_artist = tag
            .and_then(|tag| tag.get_string(ItemKey::AlbumArtist))
            .map(str::to_owned);
        let album = tag.and_then(|tag| tag.album()).map(text);

        Some(Self {
            path: path.to_string_lossy().into_owned(),
            folder: path
                .parent()
                .map(|parent| parent.to_string_lossy().into_owned())
                .unwrap_or_default(),
            album_key: album_key(album_artist.as_deref().or(artist.as_deref()), album.as_deref()),
            title,
            artist,
            album_artist,
            album,
            genre: tag.and_then(|tag| tag.genre()).map(text),
            year: tag.and_then(|tag| tag.date()).map(|date| u32::from(date.year)),
            track_no: tag.and_then(|tag| tag.track()),
            disc_no: tag.and_then(|tag| tag.disk()),
            duration_ms: Some(properties.duration().as_millis() as u64).filter(|value| *value > 0),
            sample_rate: properties.sample_rate(),
            channels: properties.channels().map(u32::from),
            bitrate_kbps: properties.audio_bitrate(),
            size_bytes,
            mtime,
        })
    }
}

fn text(value: std::borrow::Cow<'_, str>) -> String {
    value.trim().to_owned()
}

/// Ключ альбома: «исполнитель альбома + название» в нижнем регистре.
/// Считаем один раз при вставке, чтобы галерея и дерево собирались
/// простым GROUP BY, а не склейкой строк в каждом запросе.
fn album_key(artist: Option<&str>, album: Option<&str>) -> Option<String> {
    let album = album?.trim();
    if album.is_empty() {
        return None;
    }
    let artist = artist.unwrap_or("").trim().to_lowercase();
    Some(format!("{artist}\u{1f}{}", album.to_lowercase()))
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "LibraryTrack.ts")]
pub struct LibraryTrack {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub year: Option<u32>,
    pub track_no: Option<u32>,
    pub duration_ms: Option<u64>,
    pub play_count: u32,
    /// 0 — без оценки, иначе 1..5.
    pub rating: u32,
    /// Когда слушали в последний раз, unix-время. `None` — ни разу.
    pub last_played: Option<u64>,
}

/// Список колонок трека в одном месте: иначе при каждом новом поле их
/// приходилось бы догонять в трёх запросах.
pub const TRACK_COLUMNS: &str =
    "id, path, title, artist, album, year, track_no, duration_ms, play_count, rating, last_played";
/// То же с префиксом таблицы — для запросов с JOIN.
pub const TRACK_COLUMNS_T: &str = "t.id, t.path, t.title, t.artist, t.album, t.year, t.track_no, \
     t.duration_ms, t.play_count, t.rating, t.last_played";

impl LibraryTrack {
    /// Порядок полей обязан совпадать с `TRACK_COLUMNS`.
    pub fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            path: row.get(1)?,
            title: row.get(2)?,
            artist: row.get(3)?,
            album: row.get(4)?,
            year: row.get::<_, Option<i64>>(5)?.map(|value| value as u32),
            track_no: row.get::<_, Option<i64>>(6)?.map(|value| value as u32),
            duration_ms: row.get::<_, Option<i64>>(7)?.map(|value| value as u64),
            play_count: row.get::<_, i64>(8)? as u32,
            rating: row.get::<_, i64>(9)? as u32,
            last_played: row.get::<_, Option<i64>>(10)?.map(|value| value as u64),
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "LibraryStats.ts")]
pub struct LibraryStats {
    pub tracks: u32,
    pub albums: u32,
    pub artists: u32,
    pub total_duration_ms: u64,
    pub total_size_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn album_key_ignores_case_and_missing_artist() {
        assert_eq!(
            album_key(Some("Kaze No Oto"), Some("Blue Hour")),
            album_key(Some("kaze no oto"), Some("BLUE HOUR"))
        );
        assert!(album_key(None, None).is_none());
        assert!(album_key(Some("кто-то"), Some("   ")).is_none());
        assert!(album_key(None, Some("Сборник")).is_some());
    }
}
