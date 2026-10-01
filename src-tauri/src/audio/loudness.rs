//! Измерение громкости файла: декодируем целиком и кормим измеритель
//! (`dsp/loudness.rs`). Отсюда берутся собственные метки ReplayGain для
//! файлов, где их не проставили при сборке коллекции.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub use super::dsp::loudness::{Loudness, REFERENCE_LUFS};

use super::decoder::TrackSource;
use super::dsp::loudness::{Meter, album_lufs};
use super::is_stream;
use crate::error::{RingloftError, Result};

pub struct Measured {
    pub loudness: Loudness,
    /// Блоки измерения. Альбомное усиление считается по блокам всех треков
    /// сразу — усреднять готовые значения нельзя, гейт у альбома свой.
    pub blocks: Vec<f64>,
}

/// Измеряет файл. `Ok(None)` — работу отменили на полпути.
pub fn measure(path: &Path, cancel: &AtomicBool) -> Result<Option<Measured>> {
    if is_stream(path) {
        return Err(RingloftError::Decode(
            "громкость потока измерить нельзя".to_owned(),
        ));
    }

    let mut source = TrackSource::open(path)?;
    let spec = source.spec();
    let mut meter = Meter::new(spec.sample_rate, spec.channels);
    let mut block = Vec::new();
    // Отмену проверяем не на каждом блоке: час музыки — это десятки тысяч
    // блоков, а атомик здесь всё равно дешевле декодирования.
    let mut counter: u32 = 0;

    loop {
        if !source.next_block(&mut block)? {
            break;
        }
        meter.push(&block);

        counter = counter.wrapping_add(1);
        if counter.is_multiple_of(64) && cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
    }

    let blocks = meter.blocks().to_vec();
    Ok(Some(Measured {
        loudness: meter.finish(),
        blocks,
    }))
}

/// Громкость альбома по блокам всех его треков.
pub fn album_loudness(tracks: &[&Measured]) -> Loudness {
    let mut blocks = Vec::new();
    let mut peak = 0.0f32;
    for track in tracks {
        blocks.extend_from_slice(&track.blocks);
        peak = peak.max(track.loudness.peak);
    }

    Loudness {
        lufs: album_lufs(&blocks),
        peak,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::test_wav;

    /// Файл на диске должен измеряться так же, как тот же синус в памяти:
    /// синус 1 кГц даёт LUFS, равные пику в дБFS (проверено на ffmpeg ebur128).
    #[test]
    fn measures_a_file_like_the_signal_itself() {
        let path = test_wav::temp_path("loudness");
        test_wav::sine(&path, 44_100, 2, 44_100 * 6, 0.5);

        let measured = measure(&path, &AtomicBool::new(false))
            .expect("измерение")
            .expect("не отменено");
        let _ = std::fs::remove_file(&path);

        let lufs = measured.loudness.lufs.expect("громкость есть");
        assert!((lufs + 6.02).abs() < 0.4, "{lufs}");
        assert!((measured.loudness.peak - 0.5).abs() < 0.01);
        assert!(!measured.blocks.is_empty());
    }

    /// Альбом из двух одинаковых треков звучит так же, как один трек.
    #[test]
    fn album_matches_a_single_track() {
        let path = test_wav::temp_path("album");
        test_wav::sine(&path, 44_100, 2, 44_100 * 6, 0.25);

        let first = measure(&path, &AtomicBool::new(false))
            .expect("измерение")
            .expect("не отменено");
        let second = measure(&path, &AtomicBool::new(false))
            .expect("измерение")
            .expect("не отменено");
        let _ = std::fs::remove_file(&path);

        let album = album_loudness(&[&first, &second]);
        let delta = album.lufs.unwrap_or_default() - first.loudness.lufs.unwrap_or_default();
        assert!(delta.abs() < 0.01, "{album:?} против {:?}", first.loudness);
    }

    #[test]
    fn cancel_stops_the_measurement() {
        let path = test_wav::temp_path("cancel");
        test_wav::sine(&path, 44_100, 2, 44_100 * 30, 0.5);

        let result = measure(&path, &AtomicBool::new(true)).expect("измерение");
        let _ = std::fs::remove_file(&path);
        assert!(result.is_none());
    }

    #[test]
    fn stream_is_rejected() {
        let url = Path::new("https://example.org/stream");
        assert!(measure(url, &AtomicBool::new(false)).is_err());
    }
}
