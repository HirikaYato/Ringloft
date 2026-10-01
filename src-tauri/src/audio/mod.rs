//! Аудиодвижок Ringloft.
//!
//! Потоки: движок (декодирует и наполняет кольцо) и RT-колбэк cpal (только
//! читает кольцо, применяет громкость). Между ними — SPSC-кольцо f32 и набор
//! атомиков; ни одного мьютекса на пути звука.

mod decoder;
mod http_source;
mod dsp;
mod engine;
pub mod fingerprint;
pub mod loudness;
mod mix;
mod output;
mod pipeline;
mod resample;
mod ring;
mod stream;
mod tap;
#[cfg(test)]
pub(crate) mod test_wav;

pub use engine::{AudioCmd, EngineEvent, EngineHandle};

/// Интернет-радио вместо файла. Путь в плейлисте у таких строк — это адрес.
pub fn is_stream(path: &std::path::Path) -> bool {
    let text = path.to_string_lossy();
    let lower = text.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// Приводит адрес потока к виду, с которым получится собрать запрос.
///
/// В списках станций после адреса нередко пакуют описание через `|`
/// («https://host|Прохибида ла дистрибусьон|…»). Такую строку HTTP-клиент
/// отвергает уже на разборе, и до пользователя доезжает невнятное «invalid
/// uri character» — поэтому хвост отрезаем здесь, а адрес с пробелами
/// отбрасываем сразу.
pub fn clean_stream(path: &std::path::Path) -> Option<std::path::PathBuf> {
    let text = path.to_string_lossy();
    let url = text.split('|').next()?.trim();
    if url.is_empty() || url.contains(char::is_whitespace) {
        return None;
    }
    // После схемы должен быть хоть какой-то узел.
    let host = url.split_once("//")?.1;
    (!host.is_empty()).then(|| std::path::PathBuf::from(url))
}
pub use tap::TapConsumer;

pub use dsp::spectrum::Analyzer;

pub use output::{AudioDevice, list_output_devices, supported_rates};

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};

use parking_lot::RwLock;
use serde::Serialize;
use ts_rs::TS;

/// Что именно играть. Для CUE это кусок внутри общего файла, поэтому одного
/// пути мало, и поэтому же вместе с ним едет идентификатор строки плейлиста:
/// по пути такие треки не различить.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackRef {
    pub item_id: u64,
    pub path: PathBuf,
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
    /// Название из плейлиста. Для CUE только оно и верно: в теге образа
    /// лежит название диска, а не трека.
    pub title: Option<String>,
}

impl TrackRef {
    pub fn file(item_id: u64, path: PathBuf) -> Self {
        Self {
            item_id,
            path,
            start_ms: None,
            end_ms: None,
            title: None,
        }
    }

    pub fn is_region(&self) -> bool {
        self.start_ms.is_some() || self.end_ms.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "PlaybackStatus.ts")]
pub enum PlaybackStatus {
    /// Ничего не загружено.
    Idle,
    Playing,
    Paused,
    /// Трек загружен, но остановлен и перемотан в начало.
    Stopped,
}

impl PlaybackStatus {
    fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Playing,
            2 => Self::Paused,
            3 => Self::Stopped,
            _ => Self::Idle,
        }
    }

    fn as_u8(self) -> u8 {
        match self {
            Self::Idle => 0,
            Self::Playing => 1,
            Self::Paused => 2,
            Self::Stopped => 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "TrackInfo.ts")]
pub struct TrackInfo {
    /// Строка плейлиста, из которой пришёл трек. 0 — трек не из плейлиста.
    pub item_id: u64,
    /// Кусок внутри общего файла (CUE): теги файла к нему не относятся.
    pub is_region: bool,
    pub path: String,
    /// Пока это имя файла: разбор тегов появится на шаге с lofty.
    pub title: String,
    pub duration_ms: Option<u64>,
    pub sample_rate: u32,
    pub channels: u32,
    pub codec: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "PlayerState.ts")]
pub struct PlayerState {
    pub status: PlaybackStatus,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub volume: f32,
    pub track: Option<TrackInfo>,
    /// Имя устройства, на котором реально открыт поток.
    pub device: Option<String>,
    /// Частота, на которой открыт поток устройства; 0 — не открыт. Отличается
    /// от частоты трека — значит, звук пересчитывается.
    pub device_rate: u32,
}

/// Снимок состояния для UI. Пишет движок, читают команды Tauri и телеметрия.
#[derive(Debug)]
pub struct SharedState {
    status: AtomicU8,
    position_ms: AtomicU64,
    duration_ms: AtomicU64,
    volume: AtomicU32,
    track: RwLock<Option<TrackInfo>>,
    device: RwLock<Option<String>>,
    device_rate: AtomicU32,
}

impl SharedState {
    pub fn new(volume: f32) -> Self {
        Self {
            status: AtomicU8::new(PlaybackStatus::Idle.as_u8()),
            position_ms: AtomicU64::new(0),
            duration_ms: AtomicU64::new(0),
            volume: AtomicU32::new(volume.to_bits()),
            track: RwLock::new(None),
            device: RwLock::new(None),
            device_rate: AtomicU32::new(0),
        }
    }

    pub fn status(&self) -> PlaybackStatus {
        PlaybackStatus::from_u8(self.status.load(Ordering::Relaxed))
    }

    pub fn set_status(&self, status: PlaybackStatus) {
        self.status.store(status.as_u8(), Ordering::Relaxed);
    }

    pub fn position_ms(&self) -> u64 {
        self.position_ms.load(Ordering::Relaxed)
    }

    pub fn set_position_ms(&self, value: u64) {
        self.position_ms.store(value, Ordering::Relaxed);
    }

    pub fn set_duration_ms(&self, value: u64) {
        self.duration_ms.store(value, Ordering::Relaxed);
    }

    pub fn duration_ms(&self) -> u64 {
        self.duration_ms.load(Ordering::Relaxed)
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }

    pub fn set_volume(&self, value: f32) {
        self.volume.store(value.to_bits(), Ordering::Relaxed);
    }

    pub fn set_track(&self, track: Option<TrackInfo>) {
        *self.track.write() = track;
    }

    pub fn set_device(&self, device: Option<String>) {
        *self.device.write() = device;
    }

    pub fn set_device_rate(&self, rate: u32) {
        self.device_rate.store(rate, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> PlayerState {
        PlayerState {
            status: self.status(),
            position_ms: self.position_ms(),
            duration_ms: self.duration_ms(),
            volume: self.volume(),
            track: self.track.read().clone(),
            device: self.device.read().clone(),
            device_rate: self.device_rate.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{clean_stream, is_stream};

    /// Из списков станций адрес приходит с описанием после `|` — на таком
    /// HTTP-клиент спотыкается ещё на разборе.
    #[test]
    fn cleans_stream_urls() {
        assert_eq!(
            clean_stream(Path::new("https://host.example|описание|ещё")),
            Some(PathBuf::from("https://host.example"))
        );
        assert_eq!(
            clean_stream(Path::new("  http://host.example/live  ")),
            Some(PathBuf::from("http://host.example/live"))
        );
        assert_eq!(clean_stream(Path::new("https://host example/live")), None);
        assert_eq!(clean_stream(Path::new("https://")), None);
        assert_eq!(clean_stream(Path::new("|только описание")), None);
    }

    #[test]
    fn tells_streams_from_files() {
        assert!(is_stream(Path::new("http://host/live")));
        assert!(is_stream(Path::new("HTTPS://host/live")));
        assert!(!is_stream(Path::new("/music/track.flac")));
    }
}
