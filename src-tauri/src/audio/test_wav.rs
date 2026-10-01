//! Генерация WAV-файлов для тестов: чтобы не тащить в репозиторий бинарные
//! фикстуры и не зависеть от чужих файлов на диске.

use std::path::{Path, PathBuf};

/// 16-битный PCM WAV из готовых сэмплов (-1.0..1.0), interleaved.
pub fn write_wav(path: &Path, rate: u32, channels: u16, samples: &[f32]) {
    let bytes_per_sample = 2u32;
    let data_len = samples.len() as u32 * bytes_per_sample;
    let mut wav = Vec::with_capacity(44 + data_len as usize);

    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * u32::from(channels) * bytes_per_sample).to_le_bytes());
    wav.extend_from_slice(&(channels * bytes_per_sample as u16).to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());

    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
        wav.extend_from_slice(&value.to_le_bytes());
    }

    std::fs::write(path, wav).expect("запись тестового wav");
}

/// Постоянный уровень: по нему в тестах видно, чей это кусок звука.
pub fn constant(path: &Path, rate: u32, channels: u16, frames: usize, level: f32) {
    let samples = vec![level; frames * usize::from(channels)];
    write_wav(path, rate, channels, &samples);
}

/// Синус 1 кГц заданной амплитуды. Для измерения громкости постоянный
/// уровень не годится: его срезает фильтр нижних частот K-взвешивания.
pub fn sine(path: &Path, rate: u32, channels: u16, frames: usize, amplitude: f32) {
    let mut samples = Vec::with_capacity(frames * usize::from(channels));
    for frame in 0..frames {
        let phase = 2.0 * std::f64::consts::PI * 1000.0 * frame as f64 / f64::from(rate);
        let value = amplitude * phase.sin() as f32;
        for _ in 0..channels {
            samples.push(value);
        }
    }
    write_wav(path, rate, channels, &samples);
}

pub fn temp_path(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "ringloft-test-{}-{}-{name}.wav",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.subsec_nanos())
            .unwrap_or(0)
    ));
    path
}
