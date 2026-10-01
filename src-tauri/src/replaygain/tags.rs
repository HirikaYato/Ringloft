//! Чтение и запись громкостных меток. Формат общий для всех контейнеров:
//! усиление строкой «-7.32 dB», пик — линейным числом.

use std::path::Path;

use lofty::config::WriteOptions;
use lofty::file::TaggedFileExt;
use lofty::prelude::ItemKey;
use lofty::tag::{Tag, TagExt};

use crate::audio::loudness::Loudness;
use crate::error::{RingloftError, Result};

/// Что уже известно о файле до измерения: есть ли метка и к какому альбому
/// он относится. Один проход по тегам вместо двух.
#[derive(Debug, Default, Clone)]
pub struct Probe {
    pub tagged: bool,
    /// Ключ альбома — исполнитель альбома плюс название, в нижнем регистре.
    /// `None` — альбом не указан, считать альбомное усиление не по чему.
    pub album_key: Option<String>,
}

pub fn probe(path: &Path) -> Probe {
    let Ok(file) = lofty::read_from_path(path) else {
        return Probe::default();
    };
    let Some(tag) = file.primary_tag().or_else(|| file.first_tag()) else {
        return Probe::default();
    };

    let album = tag
        .get_string(ItemKey::AlbumTitle)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let artist = tag
        .get_string(ItemKey::AlbumArtist)
        .or_else(|| tag.get_string(ItemKey::TrackArtist))
        .map(str::trim)
        .unwrap_or_default();

    Probe {
        tagged: tag.get_string(ItemKey::ReplayGainTrackGain).is_some(),
        album_key: album.map(|album| {
            format!("{}\u{1}{}", artist.to_lowercase(), album.to_lowercase())
        }),
    }
}

/// Записывает метки трека и, если посчитали, альбома. Запись одна на файл:
/// каждое сохранение тега перекладывает файл целиком.
pub fn write(path: &Path, track: &Loudness, album: Option<&Loudness>) -> Result<()> {
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

    apply(tag, track, ItemKey::ReplayGainTrackGain, ItemKey::ReplayGainTrackPeak);
    match album {
        Some(album) => apply(
            tag,
            album,
            ItemKey::ReplayGainAlbumGain,
            ItemKey::ReplayGainAlbumPeak,
        ),
        // Прошлые альбомные метки уже не про этот набор треков.
        None => {
            tag.remove_key(ItemKey::ReplayGainAlbumGain);
            tag.remove_key(ItemKey::ReplayGainAlbumPeak);
        }
    }

    tag.save_to_path(path, WriteOptions::default())
        .map_err(|err| map_error(path, err))
}

fn apply(tag: &mut Tag, loudness: &Loudness, gain_key: ItemKey, peak_key: ItemKey) {
    match loudness.gain_db() {
        Some(gain) => {
            tag.insert_text(gain_key, format!("{gain:+.2} dB"));
            tag.insert_text(peak_key, format!("{:.6}", loudness.peak));
        }
        // Измерять было нечего (тишина) — врать про усиление не станем.
        None => {
            tag.remove_key(gain_key);
            tag.remove_key(peak_key);
        }
    }
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

    fn wav() -> std::path::PathBuf {
        let path = test_wav::temp_path("gain-tags");
        test_wav::sine(&path, 44_100, 2, 4_410, 0.5);
        path
    }

    #[test]
    fn writes_and_reads_back() {
        let path = wav();
        let track = Loudness {
            lufs: Some(-9.5),
            peak: 0.987_654,
        };
        write(&path, &track, None).expect("запись меток");

        let read = crate::tags::read_replay_gain(&path);
        let _ = std::fs::remove_file(&path);
        // -18 - (-9.5) = -8.5
        assert_eq!(read.track_gain_db, Some(-8.5));
        assert_eq!(read.track_peak, Some(0.987_654));
        assert_eq!(read.album_gain_db, None);
    }

    #[test]
    fn album_values_replace_old_ones() {
        let path = wav();
        let track = Loudness { lufs: Some(-9.0), peak: 0.5 };
        let album = Loudness { lufs: Some(-11.0), peak: 0.9 };
        write(&path, &track, Some(&album)).expect("запись меток");
        assert_eq!(
            crate::tags::read_replay_gain(&path).album_gain_db,
            Some(-7.0)
        );

        // Повторное измерение без альбома должно убрать прежние метки.
        write(&path, &track, None).expect("перезапись меток");
        let read = crate::tags::read_replay_gain(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(read.album_gain_db, None);
        assert_eq!(read.track_gain_db, Some(-9.0));
    }

    #[test]
    fn silence_writes_nothing() {
        let path = wav();
        let track = Loudness { lufs: Some(-9.0), peak: 0.5 };
        write(&path, &track, None).expect("запись меток");
        write(&path, &Loudness { lufs: None, peak: 0.0 }, None).expect("очистка");

        let read = crate::tags::read_replay_gain(&path);
        let probed = probe(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(read.track_gain_db, None);
        assert!(!probed.tagged);
    }

    #[test]
    fn probe_finds_the_album() {
        let path = wav();
        let edit = crate::tag_edit::TagEdit {
            album: Some("Индика".to_owned()),
            artist: Some("Back Prooff".to_owned()),
            ..Default::default()
        };
        crate::tag_edit::write(&path, &edit).expect("запись тегов");

        let probed = probe(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(probed.album_key.as_deref(), Some("back prooff\u{1}индика"));
        assert!(!probed.tagged);
    }
}
