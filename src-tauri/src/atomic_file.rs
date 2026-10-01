//! Атомарная запись JSON на диск и фоновый писатель с дебаунсом.
//!
//! Один и тот же приём нужен настройкам и плейлисту: пользователь дёргает
//! ползунок или тащит строки десятки раз в секунду, а на диск должна уйти
//! одна запись, и та — через temp + rename, чтобы падение не оставило
//! обрезанный файл.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crossbeam_channel::{Sender, TrySendError};

use crate::error::{RingloftError, Result};

pub fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|err| RingloftError::io(dir, err))?;
    }
    let tmp = path.with_extension("tmp");
    let mut file = std::fs::File::create(&tmp).map_err(|err| RingloftError::io(&tmp, err))?;
    file.write_all(contents.as_bytes())
        .map_err(|err| RingloftError::io(&tmp, err))?;
    file.sync_all().map_err(|err| RingloftError::io(&tmp, err))?;
    drop(file);
    std::fs::rename(&tmp, path).map_err(|err| RingloftError::io(path, err))
}

/// Читает JSON-файл. Отсутствие файла — это `Ok(None)`, а вот битый файл
/// откладывается в `.bad` и тоже даёт `Ok(None)`: пользователю нужнее рабочее
/// приложение, чем аварийный выход из-за одного испорченного конфига.
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
        Err(err) => {
            tracing::error!(%err, path = %path.display(), "не удалось прочитать файл");
            return None;
        }
    };

    match serde_json::from_str(&raw) {
        Ok(value) => Some(value),
        Err(err) => {
            tracing::error!(%err, path = %path.display(), "битый json, беру значения по умолчанию");
            let backup = path.with_extension("bad");
            if let Err(err) = std::fs::rename(path, &backup) {
                tracing::warn!(%err, "не удалось сохранить копию битого файла");
            }
            None
        }
    }
}

/// Фоновый писатель: копит всплеск изменений и пишет последнее состояние.
pub struct DebouncedWriter {
    path: PathBuf,
    tx: Sender<String>,
}

impl DebouncedWriter {
    pub fn new(path: PathBuf, thread_name: &str, debounce: Duration) -> Self {
        let (tx, rx) = crossbeam_channel::bounded::<String>(8);
        let writer_path = path.clone();

        let spawned = std::thread::Builder::new()
            .name(thread_name.to_owned())
            .spawn(move || {
                while let Ok(first) = rx.recv() {
                    let mut latest = first;
                    while let Ok(next) = rx.recv_timeout(debounce) {
                        latest = next;
                    }
                    if let Err(err) = write_atomic(&writer_path, &latest) {
                        tracing::error!(%err, path = %writer_path.display(), "не удалось сохранить файл");
                    }
                }
            });

        if let Err(err) = spawned {
            tracing::error!(%err, "не удалось запустить поток записи");
        }

        Self { path, tx }
    }

    pub fn schedule(&self, json: String) {
        // Очередь забита — значит писатель и так вот-вот запишет свежую версию.
        match self.tx.try_send(json) {
            Ok(()) | Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(_)) => tracing::error!("поток записи умер"),
        }
    }

    /// Синхронная запись — на выходе из приложения, когда ждать дебаунс нечем.
    pub fn flush_now(&self, json: &str) -> Result<()> {
        write_atomic(&self.path, json)
    }
}
