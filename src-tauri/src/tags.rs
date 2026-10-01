//! Теги и обложка через lofty.
//!
//! Живёт отдельно от движка намеренно: чтение тегов — это файловый ввод-вывод,
//! которому нечего делать в потоке, отвечающем за непрерывность звука. Фронт
//! запрашивает теги сам, узнав о новом треке.

use std::path::Path;

use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::PictureType;
use lofty::prelude::{Accessor, ItemKey};
use lofty::tag::Tag;
use serde::Serialize;
use ts_rs::TS;

use crate::error::{RingloftError, Result};

#[derive(Debug, Clone, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "TrackTags.ts")]
pub struct TrackTags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub year: Option<u32>,
    pub track: Option<u32>,
    pub track_total: Option<u32>,
    pub disk: Option<u32>,
    pub comment: Option<String>,
    pub duration_ms: u64,
    pub bitrate_kbps: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    /// MIME обложки. `None` — обложки в файле нет, за картинкой можно не ходить.
    pub cover_mime: Option<String>,
    /// В файле есть текст песни. Сам текст приезжает отдельной командой:
    /// в теги он попадает целыми куплетами, и таскать их в каждой строке
    /// плейлиста незачем.
    pub has_lyrics: bool,
}

/// Громкостные метки ReplayGain из тегов. Значения в децибелах, пик — в
/// линейных единицах (1.0 = полная шкала).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ReplayGain {
    pub track_gain_db: Option<f32>,
    pub track_peak: Option<f32>,
    pub album_gain_db: Option<f32>,
    pub album_peak: Option<f32>,
}

pub fn read_replay_gain(path: &Path) -> ReplayGain {
    if crate::audio::is_stream(path) {
        return ReplayGain::default();
    }
    let Ok(file) = lofty::read_from_path(path) else {
        return ReplayGain::default();
    };
    let Some(tag) = file.primary_tag().or_else(|| file.first_tag()) else {
        return ReplayGain::default();
    };

    ReplayGain {
        track_gain_db: tag.get_string(ItemKey::ReplayGainTrackGain).and_then(parse_db),
        track_peak: tag
            .get_string(ItemKey::ReplayGainTrackPeak)
            .and_then(parse_peak),
        album_gain_db: tag.get_string(ItemKey::ReplayGainAlbumGain).and_then(parse_db),
        album_peak: tag
            .get_string(ItemKey::ReplayGainAlbumPeak)
            .and_then(parse_peak),
    }
}

/// В тегах пишут по-разному: «-7.32 dB», «-7.32dB», «+3.1», иногда с запятой.
fn parse_db(value: &str) -> Option<f32> {
    let cleaned: String = value
        .trim()
        .trim_end_matches(|c: char| c.is_ascii_alphabetic() || c.is_whitespace())
        .replace(',', ".");
    cleaned.trim().parse::<f32>().ok().filter(|db| db.is_finite())
}

fn parse_peak(value: &str) -> Option<f32> {
    value
        .trim()
        .replace(',', ".")
        .parse::<f32>()
        .ok()
        .filter(|peak| peak.is_finite() && *peak > 0.0)
}

pub fn read(path: &Path) -> Result<TrackTags> {
    let file = lofty::read_from_path(path).map_err(|err| map_error(path, &err))?;
    let properties = file.properties();

    let mut tags = TrackTags {
        duration_ms: properties.duration().as_millis() as u64,
        bitrate_kbps: properties.audio_bitrate(),
        sample_rate: properties.sample_rate(),
        channels: properties.channels().map(u32::from),
        ..TrackTags::default()
    };

    let Some(tag) = file.primary_tag().or_else(|| file.first_tag()) else {
        return Ok(tags);
    };

    tags.title = tag.title().map(|value| value.to_string());
    tags.artist = tag.artist().map(|value| value.to_string());
    tags.album = tag.album().map(|value| value.to_string());
    tags.genre = tag.genre().map(|value| value.to_string());
    tags.comment = tag.comment().map(|value| value.to_string());
    tags.album_artist = tag
        .get_string(ItemKey::AlbumArtist)
        .map(|value| value.to_string());
    // Ноль в номере — не номер: так выглядит стёртое поле, когда рядом
    // остался общий счётчик (в ID3v2 это одно поле «номер/всего»).
    tags.year = tag.date().map(|date| u32::from(date.year)).filter(positive);
    tags.track = tag.track().filter(positive);
    tags.track_total = tag.track_total().filter(positive);
    tags.disk = tag.disk().filter(positive);
    tags.has_lyrics = crate::lyrics::exists(path, pick_lyrics(tag).is_some());
    tags.cover_mime = pick_cover(tag).and_then(|picture| {
        picture
            .mime_type()
            .map(|mime| mime.to_string())
            .or(Some("image/jpeg".to_owned()))
    });

    Ok(tags)
}

fn positive(value: &u32) -> bool {
    *value > 0
}

/// Текст песни из тегов. `None` — текста нет.
pub fn read_lyrics(path: &Path) -> Result<Option<String>> {
    let file = lofty::read_from_path(path).map_err(|err| map_error(path, &err))?;
    let Some(tag) = file.primary_tag().or_else(|| file.first_tag()) else {
        return Ok(None);
    };
    Ok(pick_lyrics(tag))
}

/// ID3v2 хранит текст в USLT (`UnsyncLyrics`), Vorbis и MP4 — в `LYRICS`.
/// Проверяем оба ключа: какой из них заполнен, зависит от того, чем файл
/// тегировали.
fn pick_lyrics(tag: &Tag) -> Option<String> {
    [ItemKey::UnsyncLyrics, ItemKey::Lyrics]
        .into_iter()
        .filter_map(|key| tag.get_string(key))
        .map(str::trim)
        .find(|value| !value.is_empty())
        .map(str::to_owned)
}

/// Возвращает байты обложки. Пустой вектор — обложки нет.
pub fn read_cover(path: &Path) -> Result<Vec<u8>> {
    let file = lofty::read_from_path(path).map_err(|err| map_error(path, &err))?;
    let cover = file
        .primary_tag()
        .or_else(|| file.first_tag())
        .and_then(pick_cover)
        .map(|picture| picture.data().to_vec())
        .unwrap_or_default();
    Ok(cover)
}

/// Обложка альбома, если она есть; иначе первая попавшаяся картинка.
fn pick_cover(tag: &Tag) -> Option<&lofty::picture::Picture> {
    tag.get_picture_type(PictureType::CoverFront)
        .or_else(|| tag.pictures().first())
}

fn map_error(path: &Path, err: &lofty::error::FileParseError) -> RingloftError {
    RingloftError::UnsupportedFormat(format!("{}: {err}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_file_is_an_error() {
        assert!(read(Path::new("/nope/nothing.mp3")).is_err());
    }

    /// Текст песни должен находиться и в USLT (так пишет ID3v2), и в
    /// обычном `LYRICS`, а флаг в тегах — совпадать с наличием текста.
    #[test]
    fn reads_lyrics_from_tags() {
        use lofty::config::WriteOptions;
        use lofty::tag::{Tag, TagExt, TagType};

        let path = crate::audio::test_wav::temp_path("lyrics.wav");
        crate::audio::test_wav::constant(&path, 44_100, 2, 1_000, 0.0);
        assert_eq!(read_lyrics(&path).ok().flatten(), None);
        assert!(!read(&path).map(|tags| tags.has_lyrics).unwrap_or(true));

        let mut tag = Tag::new(TagType::Id3v2);
        tag.insert_text(ItemKey::UnsyncLyrics, "  первая строка\nвторая  ".to_owned());
        tag.save_to_path(&path, WriteOptions::default())
            .expect("тег не записался");

        assert_eq!(
            read_lyrics(&path).ok().flatten().as_deref(),
            Some("первая строка\nвторая")
        );
        assert!(read(&path).map(|tags| tags.has_lyrics).unwrap_or(false));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn parses_replay_gain_notations() {
        assert_eq!(parse_db("-7.32 dB"), Some(-7.32));
        assert_eq!(parse_db("-7.32dB"), Some(-7.32));
        assert_eq!(parse_db("+3.1"), Some(3.1));
        assert_eq!(parse_db("-7,32 dB"), Some(-7.32));
        assert_eq!(parse_db("чепуха"), None);
        assert_eq!(parse_peak("0.988525"), Some(0.988_525));
        assert_eq!(parse_peak("0"), None);
    }
}
