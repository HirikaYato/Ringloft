use std::path::PathBuf;

use tauri::State;

use crate::audio::{AudioCmd, AudioDevice, PlayerState, TrackRef, list_output_devices};
use crate::error::Result;
use crate::settings::ReplayGainMode;
use crate::state::AppState;

#[tauri::command]
pub fn player_open(state: State<'_, AppState>, path: String, autoplay: bool) -> Result<()> {
    // Файл, открытый мимо плейлиста: идентификатора строки у него нет.
    state.engine.send(AudioCmd::Load {
        track: TrackRef::file(0, PathBuf::from(path)),
        autoplay,
    })
}

#[tauri::command]
pub fn player_play(state: State<'_, AppState>) -> Result<()> {
    state.engine.send(AudioCmd::Play)
}

#[tauri::command]
pub fn player_pause(state: State<'_, AppState>) -> Result<()> {
    state.engine.send(AudioCmd::Pause)
}

#[tauri::command]
pub fn player_toggle(state: State<'_, AppState>) -> Result<()> {
    state.engine.send(AudioCmd::TogglePause)
}

#[tauri::command]
pub fn player_stop(state: State<'_, AppState>) -> Result<()> {
    state.engine.send(AudioCmd::Stop)
}

#[tauri::command]
pub fn player_seek(state: State<'_, AppState>, position_ms: u64) -> Result<()> {
    state.engine.send(AudioCmd::Seek(position_ms))
}

/// Громкость сразу уходит и в движок, и в настройки: дебаунс записи на диске
/// живёт в SettingsStore, так что ползунок можно дёргать сколько угодно.
#[tauri::command]
pub fn player_set_volume(state: State<'_, AppState>, volume: f32) -> Result<()> {
    let volume = volume.clamp(0.0, 1.0);
    state.engine.send(AudioCmd::SetVolume(volume))?;
    state
        .settings
        .patch(serde_json::json!({ "playback": { "volume": volume } }))?;
    Ok(())
}

/// Режим выравнивания громкости. Настройка и движок должны меняться вместе,
/// поэтому одной командой.
#[tauri::command]
pub fn player_set_replay_gain(
    state: State<'_, AppState>,
    mode: ReplayGainMode,
    preamp_db: f32,
) -> Result<()> {
    let preamp_db = preamp_db.clamp(-15.0, 15.0);
    state.engine.send(AudioCmd::SetReplayGain {
        mode,
        preamp_db,
    })?;
    state.settings.patch(serde_json::json!({
        "audio": { "replayGain": mode, "replayGainPreampDb": preamp_db }
    }))?;
    Ok(())
}

/// Эквалайзер: настройка и движок меняются одной командой, иначе они
/// разъедутся при первом же перезапуске.
#[tauri::command]
pub fn player_set_eq(
    state: State<'_, AppState>,
    enabled: bool,
    preamp_db: f32,
    bands_db: Vec<f32>,
) -> Result<()> {
    let preamp_db = preamp_db.clamp(-15.0, 15.0);
    let bands_db: Vec<f32> = bands_db
        .into_iter()
        .map(|value| value.clamp(-15.0, 15.0))
        .collect();

    state.engine.send(AudioCmd::SetEq {
        enabled,
        preamp_db,
        bands_db: bands_db.clone(),
    })?;
    state.settings.patch(serde_json::json!({
        "audio": { "eqEnabled": enabled, "eqPreampDb": preamp_db, "eqBandsDb": bands_db }
    }))?;
    Ok(())
}

/// Длительность перекрытия между треками. 0 — бесшовный стык.
#[tauri::command]
pub fn player_set_crossfade(state: State<'_, AppState>, crossfade_ms: u32) -> Result<()> {
    let crossfade_ms = crossfade_ms.min(10_000);
    state.engine.send(AudioCmd::SetCrossfade(crossfade_ms))?;
    state
        .settings
        .patch(serde_json::json!({ "audio": { "crossfadeMs": crossfade_ms } }))?;
    Ok(())
}

#[tauri::command]
pub fn player_state(state: State<'_, AppState>) -> PlayerState {
    state.engine.snapshot()
}

#[tauri::command]
pub fn audio_devices() -> Vec<AudioDevice> {
    list_output_devices()
}

#[tauri::command]
pub fn player_set_device(state: State<'_, AppState>, device_id: Option<String>) -> Result<()> {
    state
        .engine
        .send(AudioCmd::SetDevice(device_id.clone()))?;
    state
        .settings
        .patch(serde_json::json!({ "audio": { "deviceId": device_id } }))?;
    Ok(())
}

/// Таймер сна и «остановить после этого трека». Затухание и паузу ведёт мост
/// событий; здесь только режим и следующий трек движка.
#[tauri::command]
pub fn sleep_set(
    state: State<'_, AppState>,
    request: crate::sleep::SleepRequest,
) -> Result<crate::sleep::SleepState> {
    state.sleep.set(request);
    // Заготовленный следующий трек движок сыграл бы стыком мимо остановки, а
    // снятый режим, наоборот, должен вернуть его.
    let next = if state.sleep.blocks_next() {
        None
    } else {
        let settings = state.settings.get();
        state
            .playlist
            .peek_next(settings.playback.repeat, settings.playback.shuffle)
    };
    state.engine.send(AudioCmd::SetNext(next))?;
    Ok(state.sleep.state())
}

#[tauri::command]
pub fn sleep_state(state: State<'_, AppState>) -> crate::sleep::SleepState {
    state.sleep.state()
}

/// Что с частотой: на какой открыто устройство, какие оно умеет и может ли
/// плеер разрешить PipeWire другие.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "RateStatus.ts")]
pub struct RateStatus {
    pub exact: bool,
    /// Частота играющего трека; `None` — ничего не играет.
    pub track_rate: Option<u32>,
    /// 0 — устройство не открыто.
    pub device_rate: u32,
    /// Частоты, на которых устройство откроется без пересчёта.
    pub supported: Vec<u32>,
    /// Плеер умеет менять частоты PipeWire (Linux и `pw-metadata`).
    pub can_manage: bool,
    /// Разрешение выдано плеером — лежит его фрагмент настроек.
    pub managed: bool,
}

fn rate_status(settings: &crate::settings::SettingsStore, engine: &crate::audio::EngineHandle) -> RateStatus {
    let settings = settings.get();
    let snapshot = engine.snapshot();
    RateStatus {
        exact: settings.audio.exact_rate,
        track_rate: snapshot.track.as_ref().map(|track| track.sample_rate),
        device_rate: snapshot.device_rate,
        supported: crate::audio::supported_rates(settings.audio.device_id.as_deref()),
        can_manage: crate::pipewire_rates::available(),
        managed: crate::pipewire_rates::enabled_by_us(),
    }
}

#[tauri::command]
pub async fn audio_rate_status(state: State<'_, AppState>) -> Result<RateStatus> {
    // Опрос устройств и запуск pw-metadata — не для UI-потока.
    let (settings, engine) = (std::sync::Arc::clone(&state.settings), state.engine.clone());
    super::blocking(move || Ok(rate_status(&settings, &engine))).await
}

#[tauri::command]
pub fn player_set_exact_rate(state: State<'_, AppState>, exact: bool) -> Result<()> {
    state.engine.send(AudioCmd::SetExactRate(exact))?;
    state
        .settings
        .patch(serde_json::json!({ "audio": { "exactRate": exact } }))?;
    Ok(())
}

/// Разрешить или запретить PipeWire частоты файлов. Меняет настройку всей
/// системы, поэтому зовётся только кнопкой в настройках.
#[tauri::command]
pub async fn pipewire_allow_rates(state: State<'_, AppState>, enabled: bool) -> Result<RateStatus> {
    let (settings, engine) = (std::sync::Arc::clone(&state.settings), state.engine.clone());
    super::blocking(move || {
        crate::pipewire_rates::set_enabled(enabled)?;
        Ok(rate_status(&settings, &engine))
    })
    .await
}
