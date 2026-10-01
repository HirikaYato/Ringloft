//! Системный виджет проигрывателя и медиаклавиши.
//!
//! Наружу отсюда торчит только канал обновлений: у каждой платформы свой
//! бэкенд в своём потоке (MPRIS на Linux, SMTC на Windows), и общего типа,
//! который можно было бы держать в состоянии приложения, у них нет.

use std::sync::Arc;
#[cfg(not(target_os = "linux"))]
use std::time::Duration;

use crossbeam_channel::Sender;
use tauri::{AppHandle, Manager};

use crate::audio::{AudioCmd, EngineHandle, PlaybackStatus};
use crate::playback::Controls;
use crate::playlist::PlaylistStore;
use crate::settings::{RepeatMode, SettingsStore};

/// На сколько мотает «перемотка без уточнения» из системного виджета.
/// В MPRIS шаг приходит в самом вызове, поэтому нужен только souvlaki.
#[cfg(not(target_os = "linux"))]
pub const SEEK_STEP: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct MediaContext {
    pub engine: EngineHandle,
    pub playlist: Arc<PlaylistStore>,
    pub settings: Arc<SettingsStore>,
    pub app: AppHandle,
}

#[derive(Debug, Clone, Default)]
pub struct TrackMeta {
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_ms: Option<u64>,
    /// `file://`-адрес миниатюры. Приезжает позже самих тегов: вытащить
    /// картинку — это чтение файла, и ради него не стоит задерживать
    /// показ названия.
    pub cover_url: Option<String>,
}

#[derive(Debug, Clone)]
pub enum MediaUpdate {
    Track(TrackMeta),
    Cover(Option<String>),
    Playback {
        status: PlaybackStatus,
        position_ms: u64,
    },
}

/// Что системный виджет просит сделать. Общий словарь для всех платформ:
/// бэкенды переводят в него свои события, а решение принимается один раз
/// и в одном месте.
#[derive(Debug, Clone)]
pub enum MediaAction {
    Play,
    Pause,
    Toggle,
    Stop,
    Next,
    Previous,
    /// Перемотка относительно текущей позиции, миллисекунды со знаком.
    SeekBy(i64),
    SeekTo(u64),
    SetVolume(f32),
    // Эти два действия приходят только из MPRIS: у SMTC на Windows кнопок
    // повтора и перемешивания нет, поэтому там варианты никто не создаёт.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    SetRepeat(RepeatMode),
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    SetShuffle(bool),
    Raise,
    Quit,
}

/// Пустой дескриптор означает, что системных медиаконтролей нет —
/// плеер обязан работать и без них.
#[derive(Clone, Default)]
pub struct MediaControlsHandle {
    tx: Option<Sender<MediaUpdate>>,
}

impl MediaControlsHandle {
    pub fn send(&self, update: MediaUpdate) {
        if let Some(tx) = self.tx.as_ref() {
            let _ = tx.send(update);
        }
    }
}

/// `hwnd` передаётся числом, потому что сырой указатель не Send;
/// обратно в указатель его превращает уже сам поток (нужно только Windows).
pub fn spawn(ctx: MediaContext, hwnd: Option<usize>) -> MediaControlsHandle {
    let (tx, rx) = crossbeam_channel::unbounded::<MediaUpdate>();
    let _ = hwnd;

    #[cfg(target_os = "linux")]
    let started = crate::mpris::spawn(ctx, rx);
    #[cfg(not(target_os = "linux"))]
    let started = crate::smtc::spawn(ctx, rx, hwnd);

    if started {
        MediaControlsHandle { tx: Some(tx) }
    } else {
        MediaControlsHandle::default()
    }
}

/// Единственное место, где действия системного виджета превращаются в
/// команды плеера.
pub fn handle_action(ctx: &MediaContext, action: MediaAction) {
    // Уровень info намеренно: это редкое событие, зато по логу сразу видно,
    // дошла ли медиаклавиша до плеера.
    tracing::info!(?action, "действие системных медиаконтролей");
    let controls = Controls {
        engine: &ctx.engine,
        playlist: &ctx.playlist,
        settings: &ctx.settings,
    };

    let result = match action {
        MediaAction::Play => ctx.engine.send(AudioCmd::Play),
        MediaAction::Pause => ctx.engine.send(AudioCmd::Pause),
        MediaAction::Toggle => ctx.engine.send(AudioCmd::TogglePause),
        MediaAction::Stop => ctx.engine.send(AudioCmd::Stop),
        MediaAction::Next => controls.next(true),
        MediaAction::Previous => controls.prev(),
        MediaAction::SeekBy(delta_ms) => controls.seek_relative(delta_ms),
        MediaAction::SeekTo(position_ms) => ctx.engine.send(AudioCmd::Seek(position_ms)),
        MediaAction::SetVolume(volume) => controls.set_volume(volume),
        MediaAction::SetRepeat(repeat) => {
            let shuffle = ctx.settings.get().playback.shuffle;
            controls.set_mode(repeat, shuffle).map(|_| ())
        }
        MediaAction::SetShuffle(shuffle) => {
            let repeat = ctx.settings.get().playback.repeat;
            controls.set_mode(repeat, shuffle).map(|_| ())
        }
        MediaAction::Raise => {
            show_window(&ctx.app);
            Ok(())
        }
        MediaAction::Quit => {
            ctx.app.exit(0);
            Ok(())
        }
    };

    if let Err(err) = result {
        tracing::warn!(%err, "команда от медиаклавиш не выполнена");
    }
}

/// Достаёт окно из трея: системный виджет умеет просить показать плеер.
pub fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
