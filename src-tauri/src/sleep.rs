//! Таймер сна и «остановить после этого трека».
//!
//! Оба режима гасят звук плавно — множителем затухания поверх громкости
//! (`AudioCmd::SetFade`), громкость пользователя не трогается. Решает мост
//! событий на каждом тике (`tick`): он и так знает позицию и статус, а
//! таймер во фронте в свёрнутом окне вебвью притормаживает.
//!
//! - **по времени**: последние `TIMER_FADE` звук уходит в ноль, потом пауза
//!   (не стоп — утром можно продолжить с того же места);
//! - **после трека**: последние `TRACK_FADE` трека гаснут, а следующий трек
//!   движку не готовим (`blocks_next`) — иначе бесшовный стык проскочил бы
//!   остановку.

use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

const TIMER_FADE: Duration = Duration::from_secs(12);
const TRACK_FADE_MS: u64 = 5_000;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Off,
    At(Instant),
    AfterTrack,
}

/// Что просят из интерфейса.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "SleepRequest.ts")]
pub enum SleepRequest {
    Off,
    Minutes { minutes: u32 },
    AfterTrack,
}

/// Состояние для интерфейса. Обратный отсчёт фронт ведёт сам от `remaining_ms`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "SleepState.ts")]
pub struct SleepState {
    pub kind: SleepKind,
    #[ts(type = "number | null")]
    pub remaining_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "SleepKind.ts")]
pub enum SleepKind {
    Off,
    Timer,
    AfterTrack,
}

/// Что сделать мосту после тика.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SleepAction {
    /// Ничего, кроме затухания (`fade` всегда актуален).
    None,
    /// Время вышло: пауза, затухание вернуть.
    Pause,
}

pub struct SleepTimer {
    mode: Mutex<Mode>,
    /// Последнее отправленное движку затухание — шлём только изменения.
    sent_fade: Mutex<f32>,
}

impl Default for SleepTimer {
    fn default() -> Self {
        Self {
            mode: Mutex::new(Mode::Off),
            sent_fade: Mutex::new(1.0),
        }
    }
}

impl SleepTimer {
    pub fn set(&self, request: SleepRequest) {
        *self.mode.lock() = match request {
            SleepRequest::Off => Mode::Off,
            SleepRequest::Minutes { minutes } => {
                Mode::At(Instant::now() + Duration::from_secs(u64::from(minutes.clamp(1, 24 * 60)) * 60))
            }
            SleepRequest::AfterTrack => Mode::AfterTrack,
        };
    }

    pub fn state(&self) -> SleepState {
        match *self.mode.lock() {
            Mode::Off => SleepState {
                kind: SleepKind::Off,
                remaining_ms: None,
            },
            Mode::At(deadline) => SleepState {
                kind: SleepKind::Timer,
                remaining_ms: Some(deadline.saturating_duration_since(Instant::now()).as_millis() as u64),
            },
            Mode::AfterTrack => SleepState {
                kind: SleepKind::AfterTrack,
                remaining_ms: None,
            },
        }
    }

    /// Следующий трек движку не готовим: этот должен стать последним.
    pub fn blocks_next(&self) -> bool {
        *self.mode.lock() == Mode::AfterTrack
    }

    /// Трек доиграл в режиме «после трека» — режим исполнен. `true` — значит,
    /// дальше не переходить.
    pub fn finish_track(&self) -> bool {
        let mut mode = self.mode.lock();
        if *mode == Mode::AfterTrack {
            *mode = Mode::Off;
            true
        } else {
            false
        }
    }

    /// Каждый тик моста. `fade` — каким должно быть затухание сейчас.
    pub fn tick(&self, playing: bool, position_ms: u64, duration_ms: u64) -> (f32, SleepAction) {
        let mut mode = self.mode.lock();
        match *mode {
            Mode::Off => (1.0, SleepAction::None),
            Mode::At(deadline) => {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    *mode = Mode::Off;
                    // На паузе гасить нечего — таймер просто снимается.
                    let action = if playing { SleepAction::Pause } else { SleepAction::None };
                    return (1.0, action);
                }
                (ramp(left.as_secs_f32(), TIMER_FADE.as_secs_f32()), SleepAction::None)
            }
            // У радио длительности нет — гасить по ней нечего.
            Mode::AfterTrack if duration_ms == 0 => (1.0, SleepAction::None),
            Mode::AfterTrack => {
                let left = duration_ms.saturating_sub(position_ms);
                (ramp(left as f32, TRACK_FADE_MS as f32), SleepAction::None)
            }
        }
    }

    /// Изменилось ли затухание настолько, чтобы слать его движку.
    pub fn fade_changed(&self, fade: f32) -> bool {
        let mut sent = self.sent_fade.lock();
        let changed = (fade - *sent).abs() > 0.004 || (fade == 1.0 && *sent != 1.0);
        if changed {
            *sent = fade;
        }
        changed
    }
}

/// Сколько звука оставить, когда до конца `left` из `span`. Кривая
/// квадратичная: на слух громкость падает ровнее, чем по прямой.
fn ramp(left: f32, span: f32) -> f32 {
    let linear = (left / span).clamp(0.0, 1.0);
    linear * linear
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_fades_and_then_pauses() {
        let timer = SleepTimer::default();
        *timer.mode.lock() = Mode::At(Instant::now() + Duration::from_secs(6));
        let (fade, action) = timer.tick(true, 0, 0);
        assert!(fade > 0.1 && fade < 0.4, "на середине затухания: {fade}");
        assert_eq!(action, SleepAction::None);

        *timer.mode.lock() = Mode::At(Instant::now());
        assert_eq!(timer.tick(true, 0, 0), (1.0, SleepAction::Pause));
        assert_eq!(timer.state().kind, SleepKind::Off);
    }

    #[test]
    fn far_deadline_keeps_full_volume() {
        let timer = SleepTimer::default();
        timer.set(SleepRequest::Minutes { minutes: 30 });
        assert_eq!(timer.tick(true, 0, 0).0, 1.0);
        assert_eq!(timer.state().kind, SleepKind::Timer);
    }

    #[test]
    fn after_track_fades_the_tail_and_blocks_next() {
        let timer = SleepTimer::default();
        timer.set(SleepRequest::AfterTrack);
        assert!(timer.blocks_next());
        assert_eq!(timer.tick(true, 10_000, 200_000).0, 1.0);
        let (fade, _) = timer.tick(true, 197_500, 200_000);
        assert!((fade - 0.25).abs() < 0.01, "{fade}");
        assert!(timer.finish_track());
        assert!(!timer.blocks_next());
        assert!(!timer.finish_track());
    }

    #[test]
    fn streams_are_not_faded() {
        let timer = SleepTimer::default();
        timer.set(SleepRequest::AfterTrack);
        assert_eq!(timer.tick(true, 50_000, 0).0, 1.0);
    }

    #[test]
    fn only_real_fade_changes_are_sent() {
        let timer = SleepTimer::default();
        assert!(!timer.fade_changed(1.0));
        assert!(timer.fade_changed(0.5));
        assert!(!timer.fade_changed(0.501));
        assert!(timer.fade_changed(1.0));
    }
}
