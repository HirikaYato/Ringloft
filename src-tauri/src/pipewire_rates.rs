//! Разрешённые частоты PipeWire.
//!
//! Без пересчёта звук идёт, только если PipeWire согласен переключить
//! устройство на частоту файла, а по умолчанию он разрешает одну — 48000
//! (`clock.allowed-rates`). Это настройка всей системы, поэтому плеер меняет
//! её **только по кнопке** в настройках: кладёт фрагмент в
//! `~/.config/pipewire/pipewire.conf.d/` (переживёт перезапуск) и сразу
//! применяет то же самое на ходу через `pw-metadata`. Выключение убирает файл
//! и возвращает одну частоту — ту, на которой PipeWire работает (`clock.rate`).
//!
//! На Windows общий режим WASAPI всегда смешивает на частоте системы, и здесь
//! делать нечего.

use std::path::PathBuf;
use std::process::Command;

use crate::error::{RingloftError, Result};

const FILE_NAME: &str = "50-ringloft-rates.conf";
/// Частоты, которые встречаются у файлов. Чего не умеет само устройство,
/// PipeWire не включит — он сверяет с возможностями железа.
const RATES: &str = "[ 44100 48000 88200 96000 176400 192000 ]";

fn config_file() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("pipewire/pipewire.conf.d").join(FILE_NAME))
}

/// Плеер может управлять частотами: это Linux, и `pw-metadata` на месте.
pub fn available() -> bool {
    cfg!(target_os = "linux")
        && Command::new("pw-metadata")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
}

/// Разрешение уже выдано плеером (лежит наш фрагмент).
pub fn enabled_by_us() -> bool {
    config_file().is_some_and(|path| path.is_file())
}

pub fn set_enabled(enabled: bool) -> Result<()> {
    if !available() {
        return Err(RingloftError::Internal(
            "частоты PipeWire меняются только на Linux с установленным pw-metadata".into(),
        ));
    }
    let path = config_file()
        .ok_or_else(|| RingloftError::Internal("не нашёл папку настроек пользователя".into()))?;

    if enabled {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|err| RingloftError::io(dir, err))?;
        }
        std::fs::write(
            &path,
            format!(
                "# Ringloft: разрешить PipeWire играть на частоте файла, без пересчёта.\n\
                 # Удалить файл — и всё вернётся как было (после перезапуска PipeWire).\n\
                 context.properties = {{\n    default.clock.allowed-rates = {RATES}\n}}\n"
            ),
        )
        .map_err(|err| RingloftError::io(&path, err))?;
        apply(RATES)
    } else {
        if path.is_file() {
            std::fs::remove_file(&path).map_err(|err| RingloftError::io(&path, err))?;
        }
        let rate = current_rate().unwrap_or(48_000);
        apply(&format!("[ {rate} ]"))
    }
}

/// То же самое на ходу, без перезапуска PipeWire.
fn apply(rates: &str) -> Result<()> {
    let output = Command::new("pw-metadata")
        .args(["-n", "settings", "0", "clock.allowed-rates", rates])
        .output()
        .map_err(|err| RingloftError::Internal(format!("pw-metadata не запустился: {err}")))?;
    if output.status.success() {
        tracing::info!(rates, "разрешённые частоты PipeWire изменены");
        Ok(())
    } else {
        Err(RingloftError::Internal(format!(
            "PipeWire не принял частоты: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

/// Основная частота PipeWire (`clock.rate`).
fn current_rate() -> Option<u32> {
    let output = Command::new("pw-metadata").args(["-n", "settings", "0", "clock.rate"]).output().ok()?;
    parse_value(&String::from_utf8_lossy(&output.stdout))
}

/// Из строки `update: id:0 key:'clock.rate' value:'48000' type:''`.
fn parse_value(text: &str) -> Option<u32> {
    let value = text.split("value:'").nth(1)?.split('\'').next()?;
    value.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_metadata_values() {
        let line = "Found \"settings\" metadata 31\nupdate: id:0 key:'clock.rate' value:'44100' type:''\n";
        assert_eq!(parse_value(line), Some(44_100));
        assert_eq!(parse_value("ничего"), None);
    }
}
