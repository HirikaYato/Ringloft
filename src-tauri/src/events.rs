//! Мост между бэкендом и вебвью плюс логика автоперехода.
//!
//! Здесь же живёт связка «трек кончился → взять следующий из плейлиста»:
//! движок про плейлист не знает намеренно, иначе аудиопоток пришлось бы
//! тащить к блокировкам списка.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, select};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;
use ts_rs::TS;

use crate::audio::{AudioCmd, EngineEvent, EngineHandle, PlaybackStatus, TrackInfo};
use crate::discord::{DiscordPresence, Presence};
use crate::library::{LibraryEvent, LibraryStore, file_url};
use crate::lastfm::{self, Scrobbler};
use crate::media_controls::{MediaControlsHandle, MediaUpdate, TrackMeta};
use crate::playback::Controls;
use crate::playlist::{PlaylistEvent, PlaylistStore};
use crate::settings::SettingsStore;
use crate::tray;

pub const EVENT_TICK: &str = "player:tick";
/// Радиостанция сменила название трека.
pub const EVENT_STREAM_TITLE: &str = "player:stream-title";
pub const EVENT_TRACK: &str = "player:track";
pub const EVENT_TRACK_CHANGED: &str = "player:track-changed";
pub const EVENT_STATUS: &str = "player:status";
pub const EVENT_ENDED: &str = "player:ended";
/// Таймер сна сработал или снялся сам — интерфейсу пора обновить кнопку.
pub const EVENT_SLEEP: &str = "player:sleep";
pub const EVENT_ERROR: &str = "app:error";
pub const EVENT_PLAYLIST: &str = "playlist:changed";
pub const EVENT_SCAN_PROGRESS: &str = "library:scan-progress";
pub const EVENT_SCAN_FINISHED: &str = "library:scan-finished";
pub const EVENT_GAIN_PROGRESS: &str = "gain:progress";
pub const EVENT_GAIN_FINISHED: &str = "gain:finished";
pub const EVENT_LYRICS_PROGRESS: &str = "lyrics:progress";
pub const EVENT_LYRICS_FINISHED: &str = "lyrics:finished";

/// ~15 Гц: глазу хватает, а IPC не захлёбывается.
const TICK_PERIOD: Duration = Duration::from_millis(66);
/// Как часто позиция уходит в файл сессии. Реже — обиднее терять место при
/// жёстком завершении, чаще — лишние записи на диск.
const POSITION_SAVE_PERIOD: Duration = Duration::from_secs(10);
/// MPRIS не любит поток обновлений позиции: виджеты сами её экстраполируют.
/// Раз в три секунды достаточно, чтобы не уползать.
const MEDIA_SYNC_PERIOD: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "PlayerTick.ts")]
pub struct PlayerTick {
    pub position_ms: u64,
    pub status: PlaybackStatus,
}

pub struct Bridge {
    pub engine: EngineHandle,
    pub playlist: Arc<PlaylistStore>,
    pub library: Arc<LibraryStore>,
    pub settings: Arc<SettingsStore>,
    pub media: MediaControlsHandle,
    pub scrobbler: Scrobbler,
    pub discord: DiscordPresence,
    pub sleep: Arc<crate::sleep::SleepTimer>,
}

/// Что показывать в Discord: сам трек. Позиция и статус берутся с тика, а не
/// хранятся здесь — иначе они успевали бы устареть.
#[derive(Debug, Clone, PartialEq)]
struct NowPlaying {
    title: String,
    artist: Option<String>,
    album: Option<String>,
    duration_ms: Option<u64>,
    /// Радиостанция. Ею подписываем исполнителя, когда в эфире его не
    /// назвали: «неизвестный исполнитель» для радио — неправда, просто
    /// станция об этом не сказала. У файлов здесь `None`.
    station: Option<String>,
}

/// Что сейчас слушают и засчитан ли трек. Last.fm засчитывает по времени
/// прослушивания, поэтому решение принимается на тиках, а не на стыке треков.
#[derive(Default)]
struct ScrobbleState {
    play: Option<lastfm::Play>,
    /// Путь для счётчика в библиотеке: он считается и без исполнителя, в
    /// отличие от скробблинга.
    path: Option<std::path::PathBuf>,
    done: bool,
    counted: bool,
    /// Радио: длительности у трека нет, зато видно, когда его сменили.
    stream: Option<StreamPlay>,
}

struct StreamPlay {
    play: lastfm::Play,
    since: Instant,
}

/// Меньше этого радиотрек не засчитываем: скорее всего, поймали его конец.
const MIN_STREAM_PLAY: Duration = Duration::from_secs(30);

pub fn spawn(
    app: AppHandle,
    bridge: Bridge,
    engine_events: Receiver<EngineEvent>,
    playlist_events: Receiver<PlaylistEvent>,
    library_events: Receiver<LibraryEvent>,
) {
    let spawned = std::thread::Builder::new()
        .name("ringloft-events".into())
        .spawn(move || {
            let mut last_tick = PlayerTick {
                position_ms: u64::MAX,
                status: PlaybackStatus::Idle,
            };
            let mut last_position_save = Instant::now();
            let mut last_media_sync = Instant::now();
            let mut scrobble = ScrobbleState::default();
            // Что сейчас играет — для Discord.
            let mut now: Option<NowPlaying> = None;
            // Сколько треков подряд не открылось — чтобы не крутиться вечно
            // по списку, который битый целиком.
            let mut skipped: u32 = 0;

            loop {
                select! {
                    recv(engine_events) -> message => match message {
                        Ok(event) => {
                            handle_engine(&app, &bridge, event, &mut scrobble, &mut now, &mut skipped)
                        }
                        Err(_) => break,
                    },
                    recv(playlist_events) -> message => match message {
                        Ok(PlaylistEvent::Changed) => {
                            emit(&app, EVENT_PLAYLIST, ());
                            // Список поменялся — следующий трек мог стать другим.
                            queue_next(&bridge);
                        }
                        Err(_) => break,
                    },
                    recv(library_events) -> message => match message {
                        Ok(event) => handle_library(&app, event),
                        Err(_) => break,
                    },
                    default(TICK_PERIOD) => {}
                }

                let tick = PlayerTick {
                    position_ms: bridge.engine.state().position_ms(),
                    status: bridge.engine.state().status(),
                };
                let status_changed = tick.status != last_tick.status;
                if tick.position_ms != last_tick.position_ms || status_changed {
                    emit(&app, EVENT_TICK, tick.clone());
                    bridge.playlist.set_position_ms(tick.position_ms);
                    last_tick = tick;
                }

                apply_sleep(&app, &bridge, &last_tick);

                if status_changed || last_media_sync.elapsed() >= MEDIA_SYNC_PERIOD {
                    last_media_sync = Instant::now();
                    bridge.media.send(MediaUpdate::Playback {
                        status: last_tick.status,
                        position_ms: last_tick.position_ms,
                    });
                    // Той же меркой обновляем Discord: перемотку он иначе не
                    // заметит (метки времени у него абсолютные), а сам поток
                    // отбрасывает повторы и держит свой предел частоты.
                    bridge.discord.update(discord_presence(
                        now.as_ref(),
                        last_tick.status,
                        last_tick.position_ms,
                    ));
                }

                // Порог один и тот же: трек либо прослушан, либо нет.
                if last_tick.status == PlaybackStatus::Playing
                    && (!scrobble.done || !scrobble.counted)
                    && lastfm::should_scrobble(
                        last_tick.position_ms,
                        bridge.engine.state().duration_ms(),
                    )
                {
                    if !scrobble.done
                        && let Some(play) = scrobble.play.clone()
                    {
                        bridge.scrobbler.scrobble(play);
                    }
                    scrobble.done = true;

                    if !scrobble.counted
                        && let Some(path) = scrobble.path.clone()
                    {
                        let library = Arc::clone(&bridge.library);
                        let at = lastfm::now_unix();
                        // Подпись — на момент прослушивания: итоги не должны
                        // меняться задним числом от правки тегов.
                        let record = crate::library::PlayRecord {
                            path: path.to_string_lossy().into_owned(),
                            title: now.as_ref().map(|now| now.title.clone()).unwrap_or_else(|| {
                                path.file_stem()
                                    .map(|stem| stem.to_string_lossy().into_owned())
                                    .unwrap_or_default()
                            }),
                            artist: now.as_ref().and_then(|now| now.artist.clone()),
                            album: now.as_ref().and_then(|now| now.album.clone()),
                            duration_ms: bridge.engine.state().duration_ms(),
                        };
                        // Запись в базу — не дело потока событий: он рассылает тики.
                        let spawned = std::thread::Builder::new()
                            .name("ringloft-played".into())
                            .spawn(move || library.record_play(&record, at));
                        if let Err(err) = spawned {
                            tracing::debug!(%err, "счётчик прослушиваний не обновлён");
                        }
                    }
                    scrobble.counted = true;
                }

                if last_position_save.elapsed() >= POSITION_SAVE_PERIOD {
                    last_position_save = Instant::now();
                    if last_tick.status == PlaybackStatus::Playing {
                        bridge.playlist.request_save();
                    }
                }
            }
            tracing::debug!("мост событий остановлен");
        });

    if let Err(err) = spawned {
        tracing::error!(%err, "не удалось запустить поток событий");
    }
}

/// Имя станции для подписи. В строке плейлиста у потока лежит адрес без
/// схемы («simulatorradio.stream/stream.mp3»), и путь до файла потока в
/// подписи лишний.
fn station_name(row_title: &str) -> String {
    let name = row_title.split('/').next().unwrap_or(row_title).trim();
    if name.is_empty() {
        row_title.to_owned()
    } else {
        name.to_owned()
    }
}

/// Состояние для Discord. `None` — показывать нечего.
fn discord_presence(
    now: Option<&NowPlaying>,
    status: PlaybackStatus,
    position_ms: u64,
) -> Option<Presence> {
    let track = now?;
    if status == PlaybackStatus::Idle {
        return None;
    }
    Some(Presence {
        title: track.title.clone(),
        // У радио вместо «неизвестного исполнителя» показываем станцию.
        artist: track.artist.clone().or_else(|| track.station.clone()),
        album: track.album.clone(),
        playing: status == PlaybackStatus::Playing,
        position_ms,
        duration_ms: track.duration_ms,
    })
}

/// Сколько треков подряд пропускаем, прежде чем сдаться. Весь список на
/// отключённом диске иначе крутился бы по кругу бесконечно.
const MAX_SKIPPED_IN_ROW: u32 = 20;

fn handle_engine(
    app: &AppHandle,
    bridge: &Bridge,
    event: EngineEvent,
    scrobble: &mut ScrobbleState,
    now: &mut Option<NowPlaying>,
    skipped: &mut u32,
) {
    match event {
        EngineEvent::TrackLoaded(track) => {
            *skipped = 0;
            *now = Some(announce_track(app, bridge, &track));
            start_scrobble(bridge, scrobble, &track);
            emit(app, EVENT_TRACK, track);
            queue_next(bridge);
        }
        EngineEvent::TrackChanged(track) => {
            *skipped = 0;
            // Стык прошёл без паузы: плейлист догоняет движок, а не наоборот.
            bridge.playlist.set_current_by_item(track.item_id);
            *now = Some(announce_track(app, bridge, &track));
            start_scrobble(bridge, scrobble, &track);
            emit(app, EVENT_TRACK, track.clone());
            emit(app, EVENT_TRACK_CHANGED, track);
            queue_next(bridge);
        }
        EngineEvent::StatusChanged(status) => emit(app, EVENT_STATUS, status),
        EngineEvent::StreamTitle(title) => {
            rotate_stream_play(bridge, scrobble, &title);
            // У радио название трека меняется без смены «трека»: обновляем
            // и фронт, и системный виджет, и подсказку в трее.
            tray::set_tooltip(app, &format!("Ringloft — {title}"));
            // Название из эфира приходит одной строкой «Исполнитель - Трек»:
            // не разобрав её, и виджет, и Discord показывают исполнителя
            // неизвестным, хотя станция его назвала.
            let (artist, track) = match crate::text::split_artist_title(&title) {
                Some((artist, track)) => (Some(artist), track),
                None => (None, title.clone()),
            };
            let station = now.as_ref().and_then(|current| current.station.clone());

            bridge.media.send(MediaUpdate::Track(TrackMeta {
                title: track.clone(),
                artist: artist.clone(),
                album: None,
                duration_ms: None,
                cover_url: None,
            }));
            *now = Some(NowPlaying {
                title: track,
                artist,
                album: None,
                duration_ms: None,
                station,
            });
            emit(app, EVENT_STREAM_TITLE, title);
        }
        EngineEvent::Failed(payload) => emit(app, EVENT_ERROR, payload),
        EngineEvent::LoadFailed { error, autoplay } => {
            emit(app, EVENT_ERROR, error);
            if !autoplay {
                return;
            }
            // Битый файл не останавливает музыку: переходим к следующему
            // (`skip_broken` — естественный переход без повтора одного трека).
            *skipped += 1;
            if *skipped > MAX_SKIPPED_IN_ROW {
                tracing::warn!(skipped = *skipped - 1, "подряд не открылось слишком много треков, остановился");
                *skipped = 0;
                return;
            }
            let controls = Controls {
                engine: &bridge.engine,
                playlist: &bridge.playlist,
                settings: &bridge.settings,
            };
            if let Err(err) = controls.skip_broken() {
                tracing::error!(%err, "не удалось перейти к следующему треку");
            }
        }
        EngineEvent::Ended => {
            emit(app, EVENT_ENDED, ());
            // «Остановить после этого трека»: дальше не идём, а затухание
            // возвращаем — следующий запуск должен звучать.
            if bridge.sleep.finish_track() {
                let _ = bridge.engine.send(AudioCmd::SetFade(1.0));
                bridge.sleep.fade_changed(1.0);
                emit(app, EVENT_SLEEP, bridge.sleep.state());
                queue_next(bridge);
                return;
            }
            advance(bridge);
        }
    }
}

/// Автопереход в конце трека. `manual = false`: повтор одного трека здесь
/// обязан работать, в отличие от нажатия кнопки «вперёд».
fn advance(bridge: &Bridge) {
    let controls = Controls {
        engine: &bridge.engine,
        playlist: &bridge.playlist,
        settings: &bridge.settings,
    };
    if let Err(err) = controls.next(false) {
        tracing::error!(%err, "не удалось включить следующий трек");
    }
}

/// Таймер сна на каждом тике: затухание и пауза, когда время вышло.
fn apply_sleep(app: &AppHandle, bridge: &Bridge, tick: &PlayerTick) {
    let playing = tick.status == PlaybackStatus::Playing;
    let duration_ms = bridge.engine.state().duration_ms();
    let (fade, action) = bridge.sleep.tick(playing, tick.position_ms, duration_ms);
    if action == crate::sleep::SleepAction::Pause {
        let _ = bridge.engine.send(AudioCmd::Pause);
        tracing::info!("таймер сна: пауза");
        emit(app, EVENT_SLEEP, bridge.sleep.state());
    }
    if bridge.sleep.fade_changed(fade) {
        let _ = bridge.engine.send(AudioCmd::SetFade(fade));
    }
}

/// Сообщает движку, что готовить следующим. Без этого стык треков
/// превращается в паузу на открытие файла и наполнение кольца.
fn queue_next(bridge: &Bridge) {
    let settings = bridge.settings.get();
    // Этот трек последний: заготовленный следующий движок сыграл бы стыком.
    let next = if bridge.sleep.blocks_next() {
        None
    } else {
        bridge
            .playlist
            .peek_next(settings.playback.repeat, settings.playback.shuffle)
    };
    if let Err(err) = bridge.engine.send(AudioCmd::SetNext(next)) {
        tracing::debug!(%err, "движок не принял следующий трек");
    }
}

fn handle_library(app: &AppHandle, event: LibraryEvent) {
    match event {
        LibraryEvent::Progress(progress) => emit(app, EVENT_SCAN_PROGRESS, progress),
        LibraryEvent::Finished {
            added,
            updated,
            removed,
        } => emit(
            app,
            EVENT_SCAN_FINISHED,
            serde_json::json!({ "added": added, "updated": updated, "removed": removed }),
        ),
        LibraryEvent::Failed(payload) => emit(app, EVENT_ERROR, payload),
    }
}

/// Системным контролям и трею нужны теги, а не только имя файла, поэтому
/// заглядываем в плейлист: там метаданные уже прочитаны фоновым потоком.
/// Название трека: из строки плейлиста, если она есть (у CUE там своё), иначе
/// из тега файла.
fn track_summary(bridge: &Bridge, track: &TrackInfo) -> (String, Option<String>, Option<String>) {
    match bridge
        .playlist
        .current_id()
        .and_then(|id| bridge.playlist.item_summary(id))
    {
        Some(item) => (item.0, item.1, item.2),
        None => (track.title.clone(), None, None),
    }
}

/// Радио: предыдущий трек кончился ровно тогда, когда станция назвала
/// следующий. Длительности у потока нет, поэтому засчитываем по времени
/// показа названия.
fn rotate_stream_play(bridge: &Bridge, state: &mut ScrobbleState, title: &str) {
    if let Some(previous) = state.stream.take()
        && previous.since.elapsed() >= MIN_STREAM_PLAY
    {
        bridge.scrobbler.scrobble(previous.play);
    }

    // Без исполнителя скробблинг не принимают, а в эфире его отделяют тире.
    let Some((artist, track)) = crate::text::split_artist_title(title) else {
        return;
    };
    let play = lastfm::Play {
        artist,
        track,
        album: None,
        duration_s: None,
        started_at: lastfm::now_unix(),
    };
    bridge.scrobbler.now_playing(play.clone());
    state.stream = Some(StreamPlay {
        play,
        since: Instant::now(),
    });
}

/// Начало прослушивания: «сейчас играет» уходит сразу, а зачёт — позже, когда
/// наберётся время.
fn start_scrobble(bridge: &Bridge, state: &mut ScrobbleState, track: &TrackInfo) {
    // Сменился трек — прежняя радиостанция больше не считается.
    state.stream = None;
    state.counted = false;
    // Счётчик в библиотеке живёт по пути и не зависит от тегов.
    state.path = (!crate::audio::is_stream(std::path::Path::new(&track.path)))
        .then(|| std::path::PathBuf::from(&track.path));

    let (title, artist, album) = track_summary(bridge, track);
    // Без исполнителя Last.fm скробблинг не примет.
    let Some(artist) = artist else {
        state.play = None;
        state.done = true;
        return;
    };

    let play = lastfm::Play {
        artist,
        track: title,
        album,
        duration_s: track.duration_ms.map(|ms| ms / 1_000),
        started_at: lastfm::now_unix(),
    };
    bridge.scrobbler.now_playing(play.clone());
    state.play = Some(play);
    state.done = false;
}

fn announce_track(app: &AppHandle, bridge: &Bridge, track: &TrackInfo) -> NowPlaying {
    let (title, artist, album) = track_summary(bridge, track);
    let playing = NowPlaying {
        title: title.clone(),
        artist: artist.clone(),
        album: album.clone(),
        duration_ms: track.duration_ms,
        station: crate::audio::is_stream(Path::new(&track.path)).then(|| station_name(&title)),
    };

    let tooltip = match artist.as_deref() {
        Some(artist) => format!("Ringloft — {artist} — {title}"),
        None => format!("Ringloft — {title}"),
    };
    tray::set_tooltip(app, &tooltip);

    // Текст уведомления собираем до того, как поля уедут в медиавиджет.
    let notice = (
        title.clone(),
        match (artist.as_deref(), album.as_deref()) {
            (Some(artist), Some(album)) => Some(format!("{artist} — {album}")),
            (Some(artist), None) => Some(artist.to_owned()),
            (None, Some(album)) => Some(album.to_owned()),
            (None, None) => None,
        },
    );

    bridge.media.send(MediaUpdate::Track(TrackMeta {
        title,
        artist,
        album,
        duration_ms: track.duration_ms,
        cover_url: None,
    }));

    // Обложку достаём отдельным потоком: это чтение файла и пересжатие
    // картинки, а мост событий обязан продолжать рассылать тики.
    let library = Arc::clone(&bridge.library);
    let media = bridge.media.clone();
    let path = std::path::PathBuf::from(&track.path);
    let notify = bridge.settings.get().ui.track_notifications;
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("ringloft-cover".into())
        .spawn(move || {
            let cover = (!crate::audio::is_stream(&path))
                .then(|| library.track_cover_path(&path))
                .flatten();
            media.send(MediaUpdate::Cover(cover.as_deref().map(file_url)));
            if notify {
                notify_track(&app, &notice, cover.as_deref());
            }
        });
    if let Err(err) = spawned {
        tracing::debug!(%err, "обложку для медиавиджета не достали");
    }

    playing
}

/// Всплывающее уведомление о новом треке. Шлётся из того же потока, что и
/// обложка: иконкой уведомления служит она же, а ждать её в мосте событий
/// нельзя — он обязан продолжать рассылать тики.
fn notify_track(app: &AppHandle, notice: &(String, Option<String>), cover: Option<&Path>) {
    let mut builder = app.notification().builder().title(&notice.0);
    if let Some(body) = notice.1.as_deref() {
        builder = builder.body(body);
    }
    if let Some(cover) = cover {
        builder = builder.icon(cover.to_string_lossy());
    }
    if let Err(err) = builder.show() {
        tracing::debug!(%err, "уведомление о треке не показалось");
    }
}

pub(crate) fn emit<T: Serialize + Clone>(app: &AppHandle, event: &str, payload: T) {
    if let Err(err) = app.emit(event, payload) {
        tracing::warn!(%err, event, "не удалось отправить событие во фронт");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// В подписи нужна станция, а не путь до файла потока.
    #[test]
    fn station_name_drops_the_path() {
        assert_eq!(
            station_name("simulatorradio.stream/stream.mp3"),
            "simulatorradio.stream"
        );
        assert_eq!(
            station_name("radiorecord.hostingradio.ru/rr_main96.aacp"),
            "radiorecord.hostingradio.ru"
        );
        // Название, данное руками при импорте, трогать не за что.
        assert_eq!(station_name("Радио Рекорд"), "Радио Рекорд");
        assert_eq!(station_name("/stream"), "/stream");
    }

    /// У радио исполнителя называет эфир, а если не назвал — вторую строку
    /// занимает станция: «неизвестный исполнитель» тут неправда.
    #[test]
    fn station_stands_in_for_a_missing_artist() {
        let stream = NowPlaying {
            title: "Что-то без тире".to_owned(),
            artist: None,
            album: None,
            duration_ms: None,
            station: Some("simulatorradio.stream".to_owned()),
        };
        let presence = discord_presence(Some(&stream), PlaybackStatus::Playing, 0)
            .expect("состояние есть");
        assert_eq!(presence.artist.as_deref(), Some("simulatorradio.stream"));

        // Назвал — показываем его, станция не нужна.
        let named = NowPlaying {
            artist: Some("MELL".to_owned()),
            ..stream.clone()
        };
        let presence = discord_presence(Some(&named), PlaybackStatus::Playing, 0)
            .expect("состояние есть");
        assert_eq!(presence.artist.as_deref(), Some("MELL"));

        // Остановленный плеер в профиле не висит.
        assert!(discord_presence(Some(&stream), PlaybackStatus::Idle, 0).is_none());
    }
}
