//! Системный виджет проигрывателя через souvlaki: SMTC на Windows,
//! MediaPlayer на macOS. На Linux вместо него свой MPRIS (`mpris.rs`) —
//! там от готового крейта больше вреда, чем пользы.

use std::ffi::c_void;
use std::time::Duration;

use crossbeam_channel::Receiver;
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
    SeekDirection,
};

use crate::audio::PlaybackStatus;
use crate::media_controls::{
    MediaAction, MediaContext, MediaUpdate, SEEK_STEP, TrackMeta, handle_action,
};

/// `true` — поток поднялся.
pub fn spawn(ctx: MediaContext, rx: Receiver<MediaUpdate>, hwnd: Option<usize>) -> bool {
    let spawned = std::thread::Builder::new()
        .name("ringloft-media".into())
        .spawn(move || run(ctx, rx, hwnd));

    match spawned {
        Ok(_) => true,
        Err(err) => {
            tracing::error!(%err, "не удалось запустить поток медиаконтролей");
            false
        }
    }
}

fn run(ctx: MediaContext, rx: Receiver<MediaUpdate>, hwnd: Option<usize>) {
    let config = PlatformConfig {
        dbus_name: "ringloft",
        display_name: "Ringloft",
        hwnd: hwnd.map(|value| value as *mut c_void),
    };

    let mut controls = match MediaControls::new(config) {
        Ok(controls) => controls,
        Err(err) => {
            tracing::warn!(?err, "системные медиаконтроли недоступны");
            return;
        }
    };

    if let Err(err) = controls.attach(move |event| dispatch(&ctx, event)) {
        tracing::warn!(?err, "не удалось подписаться на медиаклавиши");
        return;
    }
    let _ = controls.set_playback(MediaPlayback::Stopped);
    tracing::info!("медиаклавиши подключены");

    let mut current = TrackMeta::default();
    while let Ok(update) = rx.recv() {
        apply(&mut controls, &mut current, update);
    }
}

fn apply(controls: &mut MediaControls, current: &mut TrackMeta, update: MediaUpdate) {
    let result = match update {
        MediaUpdate::Track(meta) => {
            *current = meta;
            set_metadata(controls, current)
        }
        MediaUpdate::Cover(cover_url) => {
            current.cover_url = cover_url;
            set_metadata(controls, current)
        }
        MediaUpdate::Playback {
            status,
            position_ms,
        } => {
            let progress = Some(MediaPosition(Duration::from_millis(position_ms)));
            controls.set_playback(match status {
                PlaybackStatus::Playing => MediaPlayback::Playing { progress },
                PlaybackStatus::Paused => MediaPlayback::Paused { progress },
                _ => MediaPlayback::Stopped,
            })
        }
    };

    if let Err(err) = result {
        tracing::debug!(?err, "медиаконтроли не приняли обновление");
    }
}

fn set_metadata(controls: &mut MediaControls, meta: &TrackMeta) -> Result<(), souvlaki::Error> {
    controls.set_metadata(MediaMetadata {
        title: Some(&meta.title),
        artist: meta.artist.as_deref(),
        album: meta.album.as_deref(),
        duration: meta.duration_ms.map(Duration::from_millis),
        cover_url: meta.cover_url.as_deref(),
    })
}

/// Перевод событий souvlaki в общий словарь действий.
fn dispatch(ctx: &MediaContext, event: MediaControlEvent) {
    let action = match event {
        MediaControlEvent::Play => MediaAction::Play,
        MediaControlEvent::Pause => MediaAction::Pause,
        MediaControlEvent::Toggle => MediaAction::Toggle,
        MediaControlEvent::Stop => MediaAction::Stop,
        MediaControlEvent::Next => MediaAction::Next,
        MediaControlEvent::Previous => MediaAction::Previous,
        MediaControlEvent::SetPosition(MediaPosition(position)) => {
            MediaAction::SeekTo(position.as_millis() as u64)
        }
        MediaControlEvent::Seek(direction) => MediaAction::SeekBy(signed_ms(direction, SEEK_STEP)),
        MediaControlEvent::SeekBy(direction, amount) => {
            MediaAction::SeekBy(signed_ms(direction, amount))
        }
        MediaControlEvent::SetVolume(volume) => MediaAction::SetVolume(volume as f32),
        MediaControlEvent::Raise => MediaAction::Raise,
        MediaControlEvent::Quit => MediaAction::Quit,
        MediaControlEvent::OpenUri(uri) => {
            tracing::debug!(%uri, "OpenUri пока не поддерживается");
            return;
        }
    };
    handle_action(ctx, action);
}

fn signed_ms(direction: SeekDirection, amount: Duration) -> i64 {
    let millis = amount.as_millis() as i64;
    match direction {
        SeekDirection::Forward => millis,
        SeekDirection::Backward => -millis,
    }
}
