//! Скробблинг в Last.fm.
//!
//! Живёт в своём потоке: сеть может отвечать секундами, а мост событий обязан
//! продолжать рассылать тики. Неотправленное копится в очереди на диске —
//! иначе прослушанное за время без интернета пропадало бы.

mod api;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossbeam_channel::{RecvTimeoutError, Sender};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::atomic_file::{self};
use crate::error::{RingloftError, Result};
use crate::settings::SettingsStore;

pub use api::Credentials;

/// Как часто пробуем разгрести очередь, если в ней что-то осталось.
const RETRY_EVERY: Duration = Duration::from_secs(120);
/// Больше этого в очереди не держим: Last.fm всё равно не примет древнее.
const QUEUE_LIMIT: usize = 500;

/// Один прослушанный трек.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Play {
    pub artist: String,
    pub track: String,
    pub album: Option<String>,
    pub duration_s: Option<u64>,
    /// Момент начала прослушивания — его Last.fm и показывает в истории.
    pub started_at: u64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "LastfmStatus.ts")]
pub struct LastfmStatus {
    pub connected: bool,
    pub username: Option<String>,
    /// Сколько прослушиваний ждут отправки.
    pub pending: u32,
}

enum Message {
    NowPlaying(Play),
    Scrobble(Play),
}

/// Пустой дескриптор — скробблинг выключен или не настроен.
#[derive(Clone, Default)]
pub struct Scrobbler {
    tx: Option<Sender<Message>>,
    pending: Arc<Mutex<Vec<Play>>>,
    /// Токен, который пользователь подтверждает в браузере.
    token: Arc<Mutex<Option<String>>>,
}

impl Scrobbler {
    pub fn now_playing(&self, play: Play) {
        self.send(Message::NowPlaying(play));
    }

    pub fn scrobble(&self, play: Play) {
        self.send(Message::Scrobble(play));
    }

    fn send(&self, message: Message) {
        if let Some(tx) = self.tx.as_ref() {
            let _ = tx.send(message);
        }
    }

    pub fn status(&self, settings: &SettingsStore) -> LastfmStatus {
        let config = settings.get().lastfm;
        LastfmStatus {
            connected: config.session_key.is_some(),
            username: config.username,
            pending: self.pending.lock().len() as u32,
        }
    }

    /// Первый шаг входа: адрес, который надо открыть в браузере.
    pub fn begin_auth(&self, settings: &SettingsStore) -> Result<String> {
        let credentials = credentials(settings)?;
        let token = api::request_token(&credentials)?;
        let url = api::auth_url(&credentials, &token);
        *self.token.lock() = Some(token);
        Ok(url)
    }

    /// Второй шаг: пользователь подтвердил доступ, забираем ключ сессии.
    pub fn finish_auth(&self, settings: &SettingsStore) -> Result<LastfmStatus> {
        let credentials = credentials(settings)?;
        let Some(token) = self.token.lock().clone() else {
            return Err(RingloftError::Settings(
                "сначала откройте страницу подтверждения".to_owned(),
            ));
        };

        let (username, session_key) = api::request_session(&credentials, &token)?;
        settings.patch(serde_json::json!({
            "lastfm": { "username": username, "sessionKey": session_key, "enabled": true }
        }))?;
        *self.token.lock() = None;
        Ok(self.status(settings))
    }

    pub fn disconnect(&self, settings: &SettingsStore) -> Result<LastfmStatus> {
        settings.patch(serde_json::json!({
            "lastfm": { "username": null, "sessionKey": null, "enabled": false }
        }))?;
        Ok(self.status(settings))
    }
}

fn credentials(settings: &SettingsStore) -> Result<Credentials> {
    let config = settings.get().lastfm;
    match (config.api_key, config.api_secret) {
        (Some(api_key), Some(api_secret)) if !api_key.is_empty() && !api_secret.is_empty() => {
            Ok(Credentials {
                api_key,
                api_secret,
            })
        }
        _ => Err(RingloftError::Settings(
            "нужны ключ и секрет приложения Last.fm".to_owned(),
        )),
    }
}

/// Last.fm засчитывает трек, когда сыграна половина или четыре минуты —
/// что раньше, — и только если трек длиннее тридцати секунд.
pub fn should_scrobble(position_ms: u64, duration_ms: u64) -> bool {
    if duration_ms < 30_000 {
        return false;
    }
    position_ms >= (duration_ms / 2).min(4 * 60_000)
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0)
}

/// Поднимает поток скробблера. Пустой дескриптор — плеер работает как и
/// работал, просто молча.
pub fn spawn(settings: Arc<SettingsStore>, queue_path: PathBuf) -> Scrobbler {
    let pending: Vec<Play> = atomic_file::read_json(&queue_path).unwrap_or_default();
    let pending = Arc::new(Mutex::new(pending));
    let (tx, rx) = crossbeam_channel::unbounded::<Message>();

    let handle = Scrobbler {
        tx: Some(tx),
        pending: Arc::clone(&pending),
        token: Arc::new(Mutex::new(None)),
    };

    let spawned = std::thread::Builder::new()
        .name("ringloft-lastfm".into())
        .spawn(move || {
            loop {
                match rx.recv_timeout(RETRY_EVERY) {
                    Ok(Message::NowPlaying(play)) => send_now_playing(&settings, &play),
                    Ok(Message::Scrobble(play)) => {
                        pending.lock().push(play);
                        flush(&settings, &pending, &queue_path);
                    }
                    // Тишина — повод попробовать отправить накопленное.
                    Err(RecvTimeoutError::Timeout) => flush(&settings, &pending, &queue_path),
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        });

    match spawned {
        Ok(_) => handle,
        Err(err) => {
            tracing::error!(%err, "не удалось запустить поток Last.fm");
            Scrobbler::default()
        }
    }
}

fn session(settings: &SettingsStore) -> Option<(Credentials, String)> {
    let config = settings.get().lastfm;
    if !config.enabled {
        return None;
    }
    let credentials = Credentials {
        api_key: config.api_key?,
        api_secret: config.api_secret?,
    };
    Some((credentials, config.session_key?))
}

fn send_now_playing(settings: &SettingsStore, play: &Play) {
    let Some((credentials, session_key)) = session(settings) else {
        return;
    };
    let mut params = vec![
        ("artist", play.artist.clone()),
        ("track", play.track.clone()),
        ("sk", session_key),
    ];
    if let Some(album) = &play.album {
        params.push(("album", album.clone()));
    }
    if let Err(err) = api::call(&credentials, "track.updateNowPlaying", params, true) {
        tracing::debug!(%err, "Last.fm не принял «сейчас играет»");
    }
}

/// Отправляет накопленное. Что не ушло — остаётся в очереди до следующего
/// раза; порядок при этом сохраняется.
fn flush(settings: &SettingsStore, pending: &Mutex<Vec<Play>>, queue_path: &Path) {
    if pending.lock().is_empty() {
        return;
    }
    let Some((credentials, session_key)) = session(settings) else {
        return;
    };

    let batch: Vec<Play> = pending.lock().clone();
    let mut sent = 0;
    for play in &batch {
        let mut params = vec![
            ("artist", play.artist.clone()),
            ("track", play.track.clone()),
            ("timestamp", play.started_at.to_string()),
            ("sk", session_key.clone()),
        ];
        if let Some(album) = &play.album {
            params.push(("album", album.clone()));
        }
        if let Some(duration) = play.duration_s {
            params.push(("duration", duration.to_string()));
        }
        match api::call(&credentials, "track.scrobble", params, true) {
            Ok(_) => sent += 1,
            Err(err) => {
                tracing::debug!(%err, "скробблинг отложен");
                break;
            }
        }
    }

    if sent > 0 {
        let mut queue = pending.lock();
        let take = sent.min(queue.len());
        queue.drain(..take);
        tracing::info!(sent, left = queue.len(), "отправлено в Last.fm");
    }
    save(pending, queue_path);
}

fn save(pending: &Mutex<Vec<Play>>, queue_path: &Path) {
    let mut queue = pending.lock();
    if queue.len() > QUEUE_LIMIT {
        let extra = queue.len() - QUEUE_LIMIT;
        queue.drain(..extra);
    }
    match serde_json::to_string(&*queue) {
        Ok(json) => {
            if let Err(err) = atomic_file::write_atomic(queue_path, &json) {
                tracing::warn!(%err, "очередь Last.fm не записалась");
            }
        }
        Err(err) => tracing::warn!(%err, "очередь Last.fm не сериализовалась"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Правило Last.fm: половина трека или четыре минуты, что раньше;
    /// короче тридцати секунд — не считается вовсе.
    #[test]
    fn scrobble_threshold() {
        assert!(!should_scrobble(29_000, 29_000), "трек короче 30 секунд");
        assert!(!should_scrobble(80_000, 180_000), "меньше половины");
        assert!(should_scrobble(90_000, 180_000), "ровно половина");
        // Длинный трек засчитывается через четыре минуты, а не на середине.
        assert!(!should_scrobble(239_000, 20 * 60_000));
        assert!(should_scrobble(240_000, 20 * 60_000));
    }
}
