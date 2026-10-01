//! Хранилище настроек: чтение при старте, дебаунс-запись в фоне,
//! атомарная замена файла (temp + rename), устойчивость к битому JSON.

use std::path::PathBuf;
use std::time::Duration;

use parking_lot::RwLock;
use serde_json::Value;

use super::Settings;
use crate::atomic_file::{self, DebouncedWriter};
use crate::error::Result;

/// Сколько ждём тишины перед записью. Тянуть ползунок громкости — это сотни
/// изменений в секунду, на диск должно уйти одно.
const DEBOUNCE: Duration = Duration::from_millis(400);

pub struct SettingsStore {
    current: RwLock<Settings>,
    writer: DebouncedWriter,
}

impl SettingsStore {
    /// Загружает настройки. Любая проблема с файлом — не повод падать:
    /// берём значения по умолчанию, а битый файл откладывается в `.bad`.
    pub fn load(path: PathBuf) -> Self {
        let settings = atomic_file::read_json::<Settings>(&path)
            .map(|mut parsed| {
                parsed.normalize();
                parsed
            })
            .unwrap_or_else(|| {
                tracing::info!(path = %path.display(), "настройки по умолчанию");
                Settings::default()
            });

        Self {
            current: RwLock::new(settings),
            writer: DebouncedWriter::new(path, "ringloft-settings", DEBOUNCE),
        }
    }

    pub fn get(&self) -> Settings {
        self.current.read().clone()
    }

    /// Глубокий merge JSON-патча в текущие настройки. Патч может быть
    /// частичным на любую глубину: `{"audio":{"crossfadeMs":4000}}`.
    pub fn patch(&self, patch: Value) -> Result<Settings> {
        let updated = {
            let mut guard = self.current.write();
            let mut doc = serde_json::to_value(&*guard)?;
            merge(&mut doc, patch);
            let mut next: Settings = serde_json::from_value(doc)?;
            next.normalize();
            *guard = next.clone();
            next
        };
        self.schedule_save(&updated);
        Ok(updated)
    }

    fn schedule_save(&self, settings: &Settings) {
        match serde_json::to_string_pretty(settings) {
            Ok(json) => self.writer.schedule(json),
            Err(err) => tracing::error!(%err, "не удалось сериализовать настройки"),
        }
    }

    /// Синхронная запись — на выходе из приложения.
    pub fn flush(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&*self.current.read())?;
        self.writer.flush_now(&json)
    }
}

fn merge(dst: &mut Value, patch: Value) {
    match (dst, patch) {
        (Value::Object(dst_map), Value::Object(patch_map)) => {
            for (key, value) in patch_map {
                merge(dst_map.entry(key).or_insert(Value::Null), value);
            }
        }
        (dst, patch) => *dst = patch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_is_deep_and_partial() {
        let mut doc = serde_json::json!({"audio": {"bufferMs": 1500, "eqEnabled": false}});
        merge(&mut doc, serde_json::json!({"audio": {"eqEnabled": true}}));
        assert_eq!(doc["audio"]["bufferMs"], 1500);
        assert_eq!(doc["audio"]["eqEnabled"], true);
    }
}
