//! Системный плеер Android — бэкенд `media_controls`, как MPRIS и SMTC.
//!
//! Сам плеер (сервис переднего плана, MediaSession, уведомление, аудиофокус)
//! живёт в Kotlin, в `PlaybackService`. Отсюда туда уходят обновления, оттуда
//! сюда — нажатия, и дальше они идут в общий `handle_action`.

use std::sync::OnceLock;

use crossbeam_channel::{Receiver, Sender, select};
use jni::JNIEnv;
use jni::objects::{JClass, JObject, JValue};
use jni::sys::{jint, jlong};

use super::with_native;
use crate::audio::PlaybackStatus;
use crate::media_controls::{MediaAction, MediaContext, MediaUpdate, TrackMeta, handle_action};

/// Нажатия из Kotlin приходят на его потоках, а `handle_action` может
/// открывать файлы — поэтому они едут в наш поток через канал.
static ACTIONS: OnceLock<Sender<MediaAction>> = OnceLock::new();

/// На сколько мотают кнопки «вперёд» и «назад» на гарнитуре.
const SEEK_STEP_MS: i64 = 10_000;

/// `true` — поток поднялся.
pub fn spawn(ctx: MediaContext, updates: Receiver<MediaUpdate>) -> bool {
    let (tx, actions) = crossbeam_channel::unbounded();
    if ACTIONS.set(tx).is_err() {
        return false;
    }
    let spawned = std::thread::Builder::new()
        .name("ringloft-media".into())
        .spawn(move || run(&ctx, &updates, &actions));
    match spawned {
        Ok(_) => true,
        Err(err) => {
            tracing::error!(%err, "не удалось запустить поток системного плеера");
            false
        }
    }
}

fn run(ctx: &MediaContext, updates: &Receiver<MediaUpdate>, actions: &Receiver<MediaAction>) {
    loop {
        select! {
            recv(updates) -> update => match update {
                Ok(update) => push(update),
                Err(_) => break,
            },
            recv(actions) -> action => match action {
                Ok(action) => handle_action(ctx, action),
                Err(_) => break,
            },
        }
    }
}

fn push(update: MediaUpdate) {
    match update {
        MediaUpdate::Track(meta) => push_track(meta),
        MediaUpdate::Cover(url) => {
            // Остальным бэкендам нужен адрес `file://`, Kotlin читает файл.
            let path = url
                .as_deref()
                .and_then(crate::text::path_from_file_url)
                .map(|path| path.to_string_lossy().into_owned());
            with_native("onCover", |env, native| {
                let path = optional_string(env, path.as_deref())?;
                env.call_static_method(native, "onCover", "(Ljava/lang/String;)V", &[(&path).into()])?;
                Ok(())
            });
        }
        MediaUpdate::Playback { status, position_ms } => {
            // Те же числа — в `NowPlaying` на стороне Kotlin.
            let status = match status {
                PlaybackStatus::Playing => 1,
                PlaybackStatus::Paused => 2,
                PlaybackStatus::Idle | PlaybackStatus::Stopped => 0,
            };
            with_native("onPlayback", |env, native| {
                let args = [JValue::Int(status), JValue::Long(position_ms as jlong)];
                env.call_static_method(native, "onPlayback", "(IJ)V", &args)?;
                Ok(())
            });
        }
    }
}

fn push_track(meta: TrackMeta) {
    let cover = meta.cover_url.clone();
    with_native("onTrack", |env, native| {
        let title = env.new_string(&meta.title)?;
        let artist = optional_string(env, meta.artist.as_deref())?;
        let album = optional_string(env, meta.album.as_deref())?;
        // У радио длительности нет — системе это говорят отрицательным числом.
        let duration = meta.duration_ms.map_or(-1, |ms| ms as jlong);
        env.call_static_method(
            native,
            "onTrack",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;J)V",
            &[(&title).into(), (&artist).into(), (&album).into(), JValue::Long(duration)],
        )?;
        Ok(())
    });
    // Новый трек на той стороне сбрасывает обложку; обычно она приезжает
    // следом отдельным обновлением, но если уже известна — отдаём сразу.
    if cover.is_some() {
        push(MediaUpdate::Cover(cover));
    }
}

/// `None` уезжает в Kotlin как `null`.
fn optional_string<'local>(
    env: &mut JNIEnv<'local>,
    value: Option<&str>,
) -> jni::errors::Result<JObject<'local>> {
    match value {
        Some(value) => Ok(env.new_string(value)?.into()),
        None => Ok(JObject::null()),
    }
}

/// `Native.mediaAction` из Kotlin. Коды действий — те же, что в `Native.kt`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_hirikayato_ringloft_Native_mediaAction<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    action: jint,
    value: jlong,
) {
    let action = match action {
        0 => MediaAction::Play,
        1 => MediaAction::Pause,
        2 => MediaAction::Toggle,
        3 => MediaAction::Stop,
        4 => MediaAction::Next,
        5 => MediaAction::Previous,
        6 => MediaAction::SeekTo(value.max(0) as u64),
        7 => MediaAction::SeekBy(SEEK_STEP_MS),
        8 => MediaAction::SeekBy(-SEEK_STEP_MS),
        _ => return,
    };
    if let Some(actions) = ACTIONS.get() {
        let _ = actions.send(action);
    }
}
