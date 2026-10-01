//! Discord Rich Presence: показывает, что играет, в профиле Discord.
//!
//! Живёт в своём потоке — как и скробблер: сокет отвечает не мгновенно, а мост
//! событий обязан рассылать тики. Discord может быть не запущен вовсе, и это
//! не ошибка: тогда поток просто ничего не делает и раз в полминуты пробует
//! снова.
//!
//! Идентификатор приложения — наш, общий для всех копий плеера: его имя
//! Discord показывает в строке «слушает …». Это не секрет (секрет и токен
//! бота нам не нужны вовсе), поэтому он прямо в коде, а в настройках только
//! выключатель.

mod ipc;

use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;

use crate::settings::SettingsStore;
use ipc::Ipc;

/// Discord ограничивает частоту обновлений (примерно пять за двадцать
/// секунд), поэтому чаще, чем раз в четыре секунды, не шлём.
const MIN_INTERVAL: Duration = Duration::from_secs(4);
/// Application ID из Discord Developer Portal.
const APPLICATION_ID: &str = "1550798234820018236";
/// Пауза перед новой попыткой подключения.
const RECONNECT_AFTER: Duration = Duration::from_secs(30);
/// Как часто поток просыпается сам: чтобы отправить отложенное состояние.
const TICK: Duration = Duration::from_secs(1);

/// Что показать в профиле.
#[derive(Debug, Clone, PartialEq)]
pub struct Presence {
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub playing: bool,
    pub position_ms: u64,
    /// `None` — радио: длительности нет, время идёт вверх.
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone)]
enum Message {
    /// Новое состояние или «ничего не играет».
    Now(Option<Presence>),
}

#[derive(Clone)]
pub struct DiscordPresence {
    tx: Option<Sender<Message>>,
}

impl DiscordPresence {
    pub fn update(&self, presence: Option<Presence>) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Message::Now(presence));
        }
    }
}

/// Пробное подключение: Discord запущен и принял нас. Активность не ставит —
/// это проверка для кнопки в настройках.
pub fn probe() -> crate::error::Result<()> {
    Ipc::connect(APPLICATION_ID).map(|_| ())
}

pub fn spawn(settings: Arc<SettingsStore>) -> DiscordPresence {
    let (tx, rx) = crossbeam_channel::unbounded::<Message>();
    let handle = DiscordPresence { tx: Some(tx) };

    let spawned = std::thread::Builder::new()
        .name("ringloft-discord".into())
        .spawn(move || {
            let mut state = State::default();
            loop {
                match rx.recv_timeout(TICK) {
                    Ok(Message::Now(presence)) => {
                        state.wanted = Some(presence);
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                    // Канал закрыт — приложение выходит.
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                }
                state.flush(&settings);
            }
        });

    if let Err(err) = spawned {
        tracing::error!(%err, "не удалось запустить поток Discord");
        return DiscordPresence { tx: None };
    }
    handle
}

#[derive(Default)]
struct State {
    ipc: Option<Ipc>,
    /// Что надо показать. `None` — ничего нового не просили.
    wanted: Option<Option<Presence>>,
    /// Уже показанный кадр. Сравниваем именно кадр, а не состояние: на паузе
    /// позиция в кадр не попадает вовсе, и по состоянию мы отправляли одно и
    /// то же каждые четыре секунды.
    shown: Option<serde_json::Value>,
    last_sent: Option<Instant>,
    last_try: Option<Instant>,
}

impl State {
    fn flush(&mut self, settings: &SettingsStore) {
        if !settings.get().discord.enabled {
            // Выключили на ходу — снимаем активность и отпускаем сокет.
            if self.ipc.is_some() {
                if let Some(ipc) = &mut self.ipc {
                    let _ = ipc.set_activity(None);
                }
                self.ipc = None;
                self.shown = None;
            }
            self.wanted = None;
            return;
        }

        let Some(wanted) = self.wanted.clone() else {
            return;
        };

        let mut activity = wanted.as_ref().map(activity_json);
        if let (Some(fresh), Some(shown)) = (activity.as_mut(), self.shown.as_ref()) {
            stabilize(fresh, shown);
        }
        if activity == self.shown && self.ipc.is_some() {
            self.wanted = None;
            return;
        }
        // Подключаться только чтобы снять активность, которой мы не ставили,
        // незачем: на простое это перебор всех десяти имён сокета каждые
        // полминуты.
        if self.ipc.is_none() && wanted.is_none() {
            self.wanted = None;
            return;
        }
        if self
            .last_sent
            .is_some_and(|sent| sent.elapsed() < MIN_INTERVAL)
        {
            // Ещё рано: состояние осталось в `wanted` и уедет на следующем тике.
            return;
        }

        if self.ipc.is_none() {
            if self
                .last_try
                .is_some_and(|tried| tried.elapsed() < RECONNECT_AFTER)
            {
                return;
            }
            self.last_try = Some(Instant::now());
            match Ipc::connect(APPLICATION_ID) {
                Ok(ipc) => self.ipc = Some(ipc),
                Err(err) => {
                    tracing::debug!(%err, "Discord недоступен");
                    return;
                }
            }
        }

        let Some(ipc) = &mut self.ipc else { return };
        match ipc.set_activity(activity.clone()) {
            Ok(()) => {
                self.shown = activity;
                self.wanted = None;
                self.last_sent = Some(Instant::now());
            }
            Err(err) => {
                tracing::debug!(%err, "Discord: активность не обновилась");
                // Сокет мог закрыться вместе с Discord — подключимся заново.
                self.ipc = None;
                self.shown = None;
            }
        }
    }
}

/// Насколько метка начала может «дышать» и всё ещё считаться той же.
/// Позиция движка и настенные часы расходятся на десятки миллисекунд, а
/// перемотка сдвигает метку на секунды.
const START_TOLERANCE_MS: u64 = 2_000;

/// Оставляет прежние метки времени, если новые отличаются в пределах
/// погрешности: иначе каждый пересчёт `начало = сейчас − позиция` выглядел бы
/// как новое состояние и мы упирались бы в предел частоты Discord.
fn stabilize(fresh: &mut serde_json::Value, shown: &serde_json::Value) {
    let (Some(new_start), Some(old_start)) = (
        fresh["timestamps"]["start"].as_u64(),
        shown["timestamps"]["start"].as_u64(),
    ) else {
        return;
    };
    if new_start.abs_diff(old_start) <= START_TOLERANCE_MS {
        fresh["timestamps"] = shown["timestamps"].clone();
    }
}

/// Активность в формате Discord. Время идёт от метки начала, поэтому на паузе
/// метки не ставим вовсе — иначе счётчик продолжал бы бежать.
fn activity_json(presence: &Presence) -> serde_json::Value {
    let mut activity = serde_json::json!({
        // 2 — «слушает», иначе Discord напишет «играет в».
        "type": 2,
        "details": clamp(&presence.title),
        "state": clamp(presence.artist.as_deref().unwrap_or("Неизвестный исполнитель")),
    });

    // Альбом Discord показывает третьей строкой. У синглов он называется так
    // же, как трек, и строка просто дублировала название.
    if let Some(album) = presence
        .album
        .as_deref()
        .map(str::trim)
        .filter(|album| !album.is_empty() && !album.eq_ignore_ascii_case(presence.title.trim()))
    {
        activity["assets"] = serde_json::json!({ "large_text": clamp(album) });
    }

    if presence.playing {
        let now = unix_millis();
        let start = now.saturating_sub(presence.position_ms);
        activity["timestamps"] = match presence.duration_ms {
            Some(duration) if duration > 0 => {
                serde_json::json!({ "start": start, "end": start + duration })
            }
            // У радио конца нет: показываем только время с начала.
            _ => serde_json::json!({ "start": start }),
        };
    } else {
        activity["state"] = serde_json::json!(format!(
            "{} · пауза",
            presence.artist.as_deref().unwrap_or("Неизвестный исполнитель")
        ));
    }

    activity
}

/// Discord отвергает кадр, если строка длиннее 128 байт.
fn clamp(value: &str) -> String {
    const LIMIT: usize = 120;
    if value.len() <= LIMIT {
        return value.to_owned();
    }
    let mut end = LIMIT;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", value[..end].trim_end())
}

fn unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn presence() -> Presence {
        Presence {
            title: "Панелька".to_owned(),
            artist: Some("Хаски".to_owned()),
            album: Some("Любимые песни".to_owned()),
            playing: true,
            position_ms: 30_000,
            duration_ms: Some(201_000),
        }
    }

    #[test]
    fn playing_activity_has_both_timestamps() {
        let activity = activity_json(&presence());
        assert_eq!(activity["type"], 2);
        assert_eq!(activity["details"], "Панелька");
        assert_eq!(activity["state"], "Хаски");
        assert_eq!(activity["assets"]["large_text"], "Любимые песни");

        let start = activity["timestamps"]["start"].as_u64().expect("начало");
        let end = activity["timestamps"]["end"].as_u64().expect("конец");
        assert_eq!(end - start, 201_000);
        // Начало отсчитано назад от текущего момента на позицию в треке.
        let now = unix_millis();
        assert!(now - start >= 30_000 && now - start < 31_000, "{start} против {now}");
    }

    /// На паузе меток быть не должно: счётчик Discord продолжал бы бежать.
    #[test]
    fn paused_activity_has_no_timestamps() {
        let paused = Presence {
            playing: false,
            ..presence()
        };
        let activity = activity_json(&paused);
        assert!(activity.get("timestamps").is_none());
        assert_eq!(activity["state"], "Хаски · пауза");
    }

    /// У радио длительности нет — только время с начала.
    #[test]
    fn stream_activity_counts_up() {
        let stream = Presence {
            duration_ms: None,
            ..presence()
        };
        let activity = activity_json(&stream);
        assert!(activity["timestamps"]["start"].is_u64());
        assert!(activity["timestamps"]["end"].is_null());
    }

    /// Дрожание метки начала не должно выглядеть как новое состояние.
    #[test]
    fn small_start_drift_keeps_the_frame_equal() {
        let mut fresh = serde_json::json!({ "timestamps": { "start": 1_000_500, "end": 1_200_500 } });
        let shown = serde_json::json!({ "timestamps": { "start": 1_000_000, "end": 1_200_000 } });
        stabilize(&mut fresh, &shown);
        assert_eq!(fresh, shown, "кадр должен совпасть целиком");
    }

    /// Перемотка сдвигает метку на секунды — это уже другое состояние.
    #[test]
    fn seek_changes_the_frame() {
        let mut fresh = serde_json::json!({ "timestamps": { "start": 970_000, "end": 1_170_000 } });
        let shown = serde_json::json!({ "timestamps": { "start": 1_000_000, "end": 1_200_000 } });
        stabilize(&mut fresh, &shown);
        assert_ne!(fresh, shown);
    }

    /// На паузе меток нет вовсе, и сравнивать нечего — кадр и так совпадёт.
    #[test]
    fn paused_frames_are_equal_regardless_of_position() {
        let base = Presence {
            playing: false,
            ..presence()
        };
        let moved = Presence {
            position_ms: 90_000,
            ..base.clone()
        };
        assert_ne!(base, moved, "состояния разные");
        assert_eq!(
            activity_json(&base),
            activity_json(&moved),
            "а кадры одинаковые: позиция на паузе в кадр не попадает"
        );
    }

    #[test]
    fn long_strings_are_clamped_on_a_boundary() {
        let long = Presence {
            title: "Я".repeat(200),
            ..presence()
        };
        let activity = activity_json(&long);
        let details = activity["details"].as_str().unwrap_or_default();
        assert!(details.len() <= 124, "{}", details.len());
        assert!(details.ends_with('…'));
    }

    /// У синглов альбом называется как трек, и третья строка в карточке
    /// просто дублировала название.
    #[test]
    fn album_equal_to_title_is_dropped() {
        let single = Presence {
            title: "Изба".to_owned(),
            album: Some("Изба".to_owned()),
            ..presence()
        };
        let activity = activity_json(&single);
        assert!(activity.get("assets").is_none());

        let with_album = Presence {
            title: "Изба".to_owned(),
            album: Some("Сборник".to_owned()),
            ..presence()
        };
        assert_eq!(
            activity_json(&with_album)["assets"]["large_text"],
            "Сборник"
        );
    }

    #[test]
    fn missing_artist_has_a_placeholder() {
        let unknown = Presence {
            artist: None,
            album: None,
            ..presence()
        };
        let activity = activity_json(&unknown);
        assert_eq!(activity["state"], "Неизвестный исполнитель");
        assert!(activity.get("assets").is_none());
    }
}
