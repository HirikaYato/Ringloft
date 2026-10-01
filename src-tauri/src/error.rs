//! Единый тип ошибки бэкенда и его сериализуемое представление для фронта.
//!
//! Во фронт ошибка всегда приезжает как `{ code, message, detail? }`:
//! `code` — машиночитаемый, по нему UI решает, что показать; `message` —
//! уже готовый человеческий текст; `detail` — цепочка причин для лога и
//! раскрывающегося блока «подробности».

use std::path::PathBuf;

use serde::{Serialize, Serializer};
use ts_rs::TS;

#[derive(Debug, thiserror::Error)]
pub enum RingloftError {
    #[error("файл не найден: {}", .0.display())]
    NotFound(PathBuf),

    #[error("ошибка доступа к файлу: {}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("неподдерживаемый формат: {0}")]
    UnsupportedFormat(String),

    #[error("не удалось декодировать: {0}")]
    Decode(String),

    #[error("аудиоустройство недоступно: {0}")]
    AudioDevice(String),

    #[error("аудиодвижок не отвечает")]
    EngineGone,

    #[error("ошибка настроек: {0}")]
    Settings(String),

    #[error("ошибка базы данных: {0}")]
    Database(String),

    #[error("не удалось записать теги в {path}: {message}")]
    Tags { path: String, message: String },

    #[error("сеть недоступна: {0}")]
    Network(String),

    #[error("внутренняя ошибка: {0}")]
    Internal(String),
}

pub type Result<T, E = RingloftError> = std::result::Result<T, E>;

impl RingloftError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "NOT_FOUND",
            Self::Io { .. } => "IO",
            Self::UnsupportedFormat(_) => "UNSUPPORTED_FORMAT",
            Self::Decode(_) => "DECODE",
            Self::AudioDevice(_) => "AUDIO_DEVICE",
            Self::EngineGone => "ENGINE_GONE",
            Self::Settings(_) => "SETTINGS",
            Self::Database(_) => "DATABASE",
            Self::Tags { .. } => "TAGS",
            Self::Network(_) => "NETWORK",
            Self::Internal(_) => "INTERNAL",
        }
    }

    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub fn internal(msg: impl std::fmt::Display) -> Self {
        Self::Internal(msg.to_string())
    }

    /// Цепочка `source()` одной строкой.
    fn detail(&self) -> Option<String> {
        use std::error::Error;
        let mut parts = Vec::new();
        let mut cur = self.source();
        while let Some(err) = cur {
            parts.push(err.to_string());
            cur = err.source();
        }
        (!parts.is_empty()).then(|| parts.join(": "))
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "ErrorPayload.ts")]
pub struct ErrorPayload {
    pub code: String,
    pub message: String,
    pub detail: Option<String>,
}

impl From<&RingloftError> for ErrorPayload {
    fn from(err: &RingloftError) -> Self {
        Self {
            code: err.code().to_owned(),
            message: err.to_string(),
            detail: err.detail(),
        }
    }
}

/// Tauri требует `Serialize` от типа ошибки команды.
impl Serialize for RingloftError {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        ErrorPayload::from(self).serialize(serializer)
    }
}

impl From<anyhow::Error> for RingloftError {
    fn from(err: anyhow::Error) -> Self {
        Self::Internal(format!("{err:#}"))
    }
}

impl From<serde_json::Error> for RingloftError {
    fn from(err: serde_json::Error) -> Self {
        Self::Settings(err.to_string())
    }
}

impl From<rusqlite::Error> for RingloftError {
    fn from(err: rusqlite::Error) -> Self {
        Self::Database(err.to_string())
    }
}

impl From<tauri::Error> for RingloftError {
    fn from(err: tauri::Error) -> Self {
        Self::Internal(err.to_string())
    }
}
