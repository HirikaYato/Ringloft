//! Объект `org.mpris.MediaPlayer2.Player` — состояние и команды.
//!
//! Спецификацию соблюдаем дословно, включая живую `Position` (её менять
//! сигналом нельзя) и сигнал `Seeked`: рабочий стол судит о плеере именно
//! по этим свойствам.

use std::collections::HashMap;

use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

use crate::audio::PlaybackStatus;
use crate::media_controls::{MediaAction, MediaContext, TrackMeta, handle_action};
use crate::settings::RepeatMode;

pub(super) struct Player {
    ctx: MediaContext,
    pub(super) meta: TrackMeta,
    pub(super) status: PlaybackStatus,
    pub(super) position_us: i64,
    pub(super) track_id: u64,
}

impl Player {
    pub(super) fn new(ctx: MediaContext) -> Self {
        Self {
            ctx,
            meta: TrackMeta::default(),
            status: PlaybackStatus::Idle,
            position_us: 0,
            track_id: 0,
        }
    }
}

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    fn next(&self) {
        handle_action(&self.ctx, MediaAction::Next);
    }

    fn previous(&self) {
        handle_action(&self.ctx, MediaAction::Previous);
    }

    fn pause(&self) {
        handle_action(&self.ctx, MediaAction::Pause);
    }

    fn play_pause(&self) {
        handle_action(&self.ctx, MediaAction::Toggle);
    }

    fn stop(&self) {
        handle_action(&self.ctx, MediaAction::Stop);
    }

    fn play(&self) {
        handle_action(&self.ctx, MediaAction::Play);
    }

    fn seek(&self, offset_us: i64) {
        handle_action(&self.ctx, MediaAction::SeekBy(offset_us / 1_000));
    }

    fn set_position(&self, _track: ObjectPath<'_>, position_us: i64) {
        handle_action(&self.ctx, MediaAction::SeekTo(position_us.max(0) as u64 / 1_000));
    }

    /// Открыть трек из виджета или другой программы. Путь тот же, что у
    /// двойного щелчка в проводнике: своя вкладка, активный список не
    /// трогается.
    fn open_uri(&self, uri: String) {
        let path = if crate::audio::is_stream(std::path::Path::new(&uri)) {
            std::path::PathBuf::from(&uri)
        } else {
            match crate::text::path_from_file_url(&uri) {
                Some(path) => path,
                None => {
                    tracing::warn!(%uri, "OpenUri: адрес не разобрать");
                    return;
                }
            }
        };

        let controls = crate::playback::Controls {
            engine: &self.ctx.engine,
            playlist: &self.ctx.playlist,
            settings: &self.ctx.settings,
        };
        if let Err(err) = controls.open_paths(&[path]) {
            tracing::error!(%err, %uri, "OpenUri: не удалось включить");
        }
    }

    #[zbus(property)]
    fn playback_status(&self) -> &str {
        match self.status {
            PlaybackStatus::Playing => "Playing",
            PlaybackStatus::Paused => "Paused",
            _ => "Stopped",
        }
    }

    #[zbus(property)]
    fn loop_status(&self) -> &str {
        match self.ctx.settings.get().playback.repeat {
            RepeatMode::Off => "None",
            RepeatMode::Track => "Track",
            RepeatMode::All => "Playlist",
        }
    }

    #[zbus(property)]
    fn set_loop_status(&self, value: &str) {
        let repeat = match value {
            "Track" => RepeatMode::Track,
            "Playlist" => RepeatMode::All,
            _ => RepeatMode::Off,
        };
        handle_action(&self.ctx, MediaAction::SetRepeat(repeat));
    }

    #[zbus(property)]
    fn shuffle(&self) -> bool {
        self.ctx.settings.get().playback.shuffle
    }

    #[zbus(property)]
    fn set_shuffle(&self, value: bool) {
        handle_action(&self.ctx, MediaAction::SetShuffle(value));
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        metadata(&self.meta, self.track_id)
    }

    #[zbus(property)]
    fn volume(&self) -> f64 {
        f64::from(self.ctx.settings.get().playback.volume)
    }

    #[zbus(property)]
    fn set_volume(&self, value: f64) {
        handle_action(&self.ctx, MediaAction::SetVolume(value.clamp(0.0, 1.0) as f32));
    }

    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> i64 {
        self.position_us
    }

    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn set_rate(&self, _value: f64) {}

    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }

    #[zbus(signal)]
    async fn seeked(emitter: &SignalEmitter<'_>, position_us: i64) -> zbus::Result<()>;
}

/// Метаданные трека в словаре MPRIS. Неупаковавшееся поле пропускаем:
/// виджет без альбома лучше, чем виджет без метаданных вообще.
fn metadata(meta: &TrackMeta, track_id: u64) -> HashMap<String, OwnedValue> {
    let mut map = HashMap::new();
    let path = format!("/ringloft/track/{track_id}");
    if let Ok(path) = ObjectPath::try_from(path) {
        put(&mut map, "mpris:trackid", path);
    }
    put(&mut map, "xesam:title", meta.title.clone());
    if let Some(artist) = meta.artist.clone() {
        put(&mut map, "xesam:artist", vec![artist]);
    }
    if let Some(album) = meta.album.clone() {
        put(&mut map, "xesam:album", album);
    }
    if let Some(duration_ms) = meta.duration_ms {
        put(&mut map, "mpris:length", duration_ms as i64 * 1_000);
    }
    if let Some(cover) = meta.cover_url.clone() {
        put(&mut map, "mpris:artUrl", cover);
    }
    map
}

fn put<'a, T: Into<Value<'a>>>(map: &mut HashMap<String, OwnedValue>, key: &str, value: T) {
    match OwnedValue::try_from(value.into()) {
        Ok(value) => {
            map.insert(key.to_owned(), value);
        }
        Err(err) => tracing::debug!(%err, key, "поле метаданных не упаковалось"),
    }
}
