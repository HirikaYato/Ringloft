//! Поток аудиодвижка: владеет конвейером, кольцом и потоком cpal.
//! Общение только через канал команд; наружу — события и атомики состояния.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, TryRecvError};

use super::decoder::TrackSource;
use super::output::{self, OutputStream};
use super::pipeline::{EqSettings, Pipeline, PumpOutcome};
use super::ring::{self, RingProducer, RingShared};
use super::tap::{self, TapConsumer};
use super::{PlaybackStatus, PlayerState, SharedState, TrackInfo, TrackRef};
use crate::error::{ErrorPayload, RingloftError, Result};
use crate::settings::{ReplayGainMode, Settings};
use crate::tags;

/// Сколько спим, когда кольцо заполнено и команд нет.
const IDLE_TICK: Duration = Duration::from_millis(5);
/// Минимальная ёмкость кольца в сэмплах — страховка от абсурдных настроек.
const MIN_RING_SAMPLES: usize = 8192;
/// Ёмкость отвода: около четверти секунды стерео на 48 кГц. Больше незачем,
/// анализатору нужен свежий звук, а не архив.
const TAP_SAMPLES: usize = 24_576;

#[derive(Debug)]
pub enum AudioCmd {
    Load { track: TrackRef, autoplay: bool },
    Play,
    Pause,
    TogglePause,
    Stop,
    Seek(u64),
    /// Что играть сразу за текущим треком, без паузы. `None` — ничего.
    SetNext(Option<TrackRef>),
    SetReplayGain {
        mode: ReplayGainMode,
        preamp_db: f32,
    },
    SetEq {
        enabled: bool,
        preamp_db: f32,
        bands_db: Vec<f32>,
    },
    /// 0 — бесшовный стык, иначе длительность перекрытия.
    SetCrossfade(u32),
    SetVolume(f32),
    /// Частота файла без пересчёта, когда устройство её умеет.
    SetExactRate(bool),
    /// Профиль наушников (AutoEQ); `None` — выключен.
    SetHeadphone(Option<crate::settings::HeadphoneProfile>),
    /// Затухание поверх громкости, 0…1: таймер сна гасит звук им, не трогая
    /// громкость пользователя.
    SetFade(f32),
    SetDevice(Option<String>),
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum EngineEvent {
    TrackLoaded(TrackInfo),
    /// Заиграл следующий трек — стык прошли без паузы.
    TrackChanged(TrackInfo),
    StatusChanged(PlaybackStatus),
    /// Радиостанция сменила название трека прямо в потоке.
    StreamTitle(String),
    Ended,
    Failed(ErrorPayload),
    /// Трек не открылся: файла нет, он битый или формат не тот. Отдельно от
    /// `Failed`, потому что решение тут другое — такой трек пропускают, а не
    /// останавливают на нём воспроизведение.
    LoadFailed {
        error: ErrorPayload,
        /// Трек собирались играть сразу. При восстановлении сессии — нет, и
        /// тогда перескакивать некуда и незачем.
        autoplay: bool,
    },
}

/// Граница трека внутри кольца: в кольце одновременно лежит хвост одного
/// трека и начало следующего.
#[derive(Debug, Clone, Default)]
struct Marker {
    /// Кадр кольца, с которого начинается этот трек.
    start_frame: u64,
    track: Option<TrackInfo>,
    duration_ms: u64,
    /// Позиция внутри трека, соответствующая `start_frame`.
    /// Ненулевая после перемотки.
    segment_start_ms: u64,
}

#[derive(Clone)]
pub struct EngineHandle {
    commands: Sender<AudioCmd>,
    state: Arc<SharedState>,
    /// Пока флаг снят, колбэк не тратит время на отвод для спектра.
    analysis: Arc<AtomicBool>,
}

impl EngineHandle {
    pub fn spawn(
        settings: &Settings,
    ) -> (Self, Receiver<EngineEvent>, Receiver<TapConsumer>) {
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
        let (event_tx, event_rx) = crossbeam_channel::unbounded();
        let (tap_tx, tap_rx) = crossbeam_channel::unbounded();
        let analysis = Arc::new(AtomicBool::new(false));

        let volume = if settings.playback.muted {
            0.0
        } else {
            settings.playback.volume
        };
        let state = Arc::new(SharedState::new(volume));
        let mut engine = Engine::new(cmd_rx, event_tx, Arc::clone(&state), settings);
        engine.tap_tx = Some(tap_tx);
        engine.analysis = Arc::clone(&analysis);
        engine.pipeline.set_eq(EqSettings {
            enabled: settings.audio.eq_enabled,
            preamp_db: settings.audio.eq_preamp_db,
            bands_db: settings.audio.eq_bands_db.clone(),
        });
        engine.pipeline.set_crossfade(settings.audio.crossfade_ms);
        engine.pipeline.set_headphone(
            settings
                .audio
                .headphone
                .clone()
                .filter(|_| settings.audio.headphone_enabled),
        );

        if let Err(err) = std::thread::Builder::new()
            .name("ringloft-audio".into())
            .spawn(move || engine.run())
        {
            tracing::error!(%err, "не удалось запустить поток аудиодвижка");
        }

        (
            Self {
                commands: cmd_tx,
                state,
                analysis,
            },
            event_rx,
            tap_rx,
        )
    }

    /// Включает и выключает отвод звука для визуализатора.
    pub fn set_analysis(&self, enabled: bool) {
        self.analysis
            .store(enabled, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn send(&self, cmd: AudioCmd) -> Result<()> {
        self.commands.send(cmd).map_err(|_| RingloftError::EngineGone)
    }

    pub fn snapshot(&self) -> PlayerState {
        self.state.snapshot()
    }

    pub fn state(&self) -> &Arc<SharedState> {
        &self.state
    }
}

/// Открывает источник целиком или куском — для CUE.
fn open_source(track: &TrackRef) -> Result<TrackSource> {
    match track.is_region() {
        true => TrackSource::open_region(
            &track.path,
            track.start_ms.unwrap_or(0),
            track.end_ms,
        ),
        false => TrackSource::open(&track.path),
    }
}

fn track_info(track: &TrackRef, source: &TrackSource) -> TrackInfo {
    let spec = source.spec();
    let path = track.path.as_path();
    TrackInfo {
        item_id: track.item_id,
        is_region: track.is_region(),
        path: path.display().to_string(),
        title: track.title.clone().unwrap_or_else(|| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("без названия")
                .to_owned()
        }),
        duration_ms: source.duration_ms(),
        sample_rate: spec.sample_rate,
        channels: spec.channels as u32,
        codec: source.codec_name().to_owned(),
    }
}

struct Engine {
    commands: Receiver<AudioCmd>,
    events: Sender<EngineEvent>,
    state: Arc<SharedState>,

    device_id: Option<String>,
    buffer_ms: u32,
    replay_gain: ReplayGainMode,
    replay_gain_preamp_db: f32,
    device_rate: u32,
    device_channels: usize,

    output: Option<OutputStream>,
    producer: Option<RingProducer>,
    ring: Option<Arc<RingShared>>,

    pipeline: Pipeline,
    current_track: Option<TrackRef>,
    next_track: Option<TrackRef>,
    /// Следующий трек, который заранее открыть не вышло. Мост событий
    /// присылает его снова на каждое обновление плейлиста, и без этой
    /// пометки битый файл перечитывался бы раз за разом — а на сетевой
    /// папке ещё и подвешивал поток движка.
    failed_next: Option<TrackRef>,
    /// Последнее название, пришедшее из потока радио: шлём событие только
    /// когда оно меняется.
    stream_title: Option<String>,
    /// Что играет прямо сейчас и с какого кадра кольца оно началось.
    current: Marker,
    /// Границы треков внутри кольца: в нём одновременно лежит хвост одного
    /// трека и начало следующего, поэтому «позиция = сыгранные кадры» неверна.
    markers: VecDeque<Marker>,
    /// Кадров записано в кольцо с последнего сброса — система координат маркеров.
    frames_written: u64,
    /// Описание трека, который лежит в конвейере «на очереди».
    next_info: Option<TrackInfo>,
    ended: bool,
    /// Кольцо после загрузки или перемотки получило первые данные. До этого
    /// пустое кольцо — обычная тишина старта, а не пропуск.
    primed: bool,
    /// Сколько пропусков насчитало кольцо к началу текущего трека: счётчик в
    /// кольце общий на всё время жизни устройства.
    underrun_mark: u64,

    error_tx: Sender<String>,
    error_rx: Receiver<String>,
    /// Куда отдавать потребителя отвода при каждом открытии устройства.
    tap_tx: Option<Sender<TapConsumer>>,
    analysis: Arc<AtomicBool>,
    /// Множитель затухания таймера сна; новое кольцо получает его же.
    fade: f32,
    /// Открывать устройство на частоте файла (`audio.exact_rate`).
    exact_rate: bool,
    /// Следующий трек другой частоты: заранее его не открываем (стык всё
    /// равно не выйдет — устройство надо переоткрыть) и не проверяем заново
    /// на каждое обновление плейлиста.
    rate_skipped_next: Option<TrackRef>,
}

impl Engine {
    fn new(
        commands: Receiver<AudioCmd>,
        events: Sender<EngineEvent>,
        state: Arc<SharedState>,
        settings: &Settings,
    ) -> Self {
        let (error_tx, error_rx) = crossbeam_channel::bounded(8);
        Self {
            commands,
            events,
            state,
            device_id: settings.audio.device_id.clone(),
            buffer_ms: settings.audio.buffer_ms,
            exact_rate: settings.audio.exact_rate,
            rate_skipped_next: None,
            replay_gain: settings.audio.replay_gain,
            replay_gain_preamp_db: settings.audio.replay_gain_preamp_db,
            device_rate: 0,
            device_channels: 0,
            output: None,
            producer: None,
            ring: None,
            pipeline: Pipeline::new(),
            stream_title: None,
            current_track: None,
            next_track: None,
            failed_next: None,
            current: Marker::default(),
            markers: VecDeque::new(),
            frames_written: 0,
            next_info: None,
            ended: false,
            primed: false,
            underrun_mark: 0,
            error_tx,
            error_rx,
            tap_tx: None,
            analysis: Arc::new(AtomicBool::new(false)),
            fade: 1.0,
        }
    }

    fn run(mut self) {
        tracing::info!("аудиодвижок запущен");
        loop {
            match self.commands.try_recv() {
                Ok(cmd) => {
                    if !self.handle(cmd) {
                        break;
                    }
                    continue;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => break,
            }

            self.drain_device_errors();
            let progressed = self.pump();
            self.update_position();
            self.check_stream_title();
            self.check_finished();

            if !progressed {
                match self.commands.recv_timeout(IDLE_TICK) {
                    Ok(cmd) => {
                        if !self.handle(cmd) {
                            break;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        }
        self.set_playing(false);
        tracing::info!("аудиодвижок остановлен");
    }

    /// У радио название трека приходит в самом потоке и меняется на ходу.
    fn check_stream_title(&mut self) {
        let title = self.pipeline.stream_title();
        if title.is_some() && title != self.stream_title {
            self.stream_title = title.clone();
            if let Some(title) = title {
                self.emit(EngineEvent::StreamTitle(title));
            }
        }
    }

    /// `false` — выходим из цикла.
    fn handle(&mut self, cmd: AudioCmd) -> bool {
        tracing::debug!(?cmd, "команда движка");
        match cmd {
            AudioCmd::Load { track, autoplay } => {
                self.stream_title = None;
                self.load(&track, autoplay)
            }
            AudioCmd::Play => self.play(),
            AudioCmd::Pause => self.set_playing(false),
            AudioCmd::TogglePause => {
                if self.state.status() == PlaybackStatus::Playing {
                    self.set_playing(false);
                } else {
                    self.play();
                }
            }
            AudioCmd::Stop => self.stop(),
            AudioCmd::Seek(position_ms) => self.seek(position_ms),
            AudioCmd::SetNext(track) => self.set_next(track),
            AudioCmd::SetCrossfade(ms) => self.pipeline.set_crossfade(ms),
            AudioCmd::SetEq {
                enabled,
                preamp_db,
                bands_db,
            } => self.pipeline.set_eq(EqSettings {
                enabled,
                preamp_db,
                bands_db,
            }),
            AudioCmd::SetReplayGain { mode, preamp_db } => {
                self.replay_gain = mode;
                self.replay_gain_preamp_db = preamp_db.clamp(-15.0, 15.0);
                if let Some(track) = self.current_track.clone() {
                    let gain = self.replay_gain_factor(&track.path);
                    self.pipeline.set_gain(gain);
                }
            }
            AudioCmd::SetVolume(volume) => {
                let volume = volume.clamp(0.0, 1.0);
                self.state.set_volume(volume);
                if let Some(ring) = self.ring.as_ref() {
                    ring.set_volume(volume);
                }
            }
            AudioCmd::SetHeadphone(profile) => self.pipeline.set_headphone(profile),
            AudioCmd::SetExactRate(exact) => {
                self.exact_rate = exact;
                self.rate_skipped_next = None;
            }
            AudioCmd::SetFade(fade) => {
                self.fade = fade.clamp(0.0, 1.0);
                if let Some(ring) = self.ring.as_ref() {
                    ring.set_fade(self.fade);
                }
            }
            AudioCmd::SetDevice(device_id) => self.switch_device(device_id),
            AudioCmd::Shutdown => {
                self.report_underruns();
                return false;
            }
        }
        true
    }

    fn pump(&mut self) -> bool {
        let Some(producer) = self.producer.as_mut() else {
            return false;
        };
        let channels = self.device_channels.max(1);
        match self.pipeline.pump(producer) {
            PumpOutcome::Wrote {
                samples,
                started_next,
            } => {
                if started_next && let Some(track) = self.next_info.take() {
                    let duration_ms = track.duration_ms.unwrap_or(0);
                    self.markers.push_back(Marker {
                        start_frame: self.frames_written,
                        track: Some(track),
                        duration_ms,
                        segment_start_ms: 0,
                    });
                }
                self.frames_written += (samples / channels) as u64;
                if !self.primed {
                    self.primed = true;
                    if let Some(ring) = self.ring.as_ref() {
                        ring.set_expecting_data(true);
                    }
                }
                true
            }
            PumpOutcome::Idle => false,
            PumpOutcome::Drained => {
                // Источник кончился и всё уже в кольце: дальше пустое кольцо —
                // это конец трека, а не пропуск данных.
                if let Some(ring) = self.ring.as_ref() {
                    ring.set_expecting_data(false);
                }
                false
            }
            PumpOutcome::Failed(err) => {
                self.report(&err);
                false
            }
        }
    }

    fn load(&mut self, track: &TrackRef, autoplay: bool) {
        // Новый трек — новые попытки: битый «следующий» могли успеть починить,
        // а частота следующего сравнивается уже с новым устройством.
        self.failed_next = None;
        self.rate_skipped_next = None;
        match self.load_inner(track) {
            Ok(info) => {
                // Порядок важен: фронт сначала узнаёт трек, потом статус.
                self.emit(EngineEvent::TrackLoaded(info));
                self.set_playing(autoplay);
                if !autoplay {
                    self.state.set_status(PlaybackStatus::Paused);
                }
            }
            Err(err) => {
                self.pipeline.clear();
                self.state.set_track(None);
                self.state.set_status(PlaybackStatus::Idle);
                tracing::error!(%err, path = %track.path.display(), "трек не открылся");
                self.emit(EngineEvent::LoadFailed {
                    error: ErrorPayload::from(&err),
                    autoplay,
                });
            }
        }
    }

    fn load_inner(&mut self, track: &TrackRef) -> Result<TrackInfo> {
        let path = track.path.as_path();
        let source = open_source(track)?;
        let spec = source.spec();
        if self.needs_rate_switch(spec.sample_rate) {
            // Устройство открыто на другой частоте, а файл оно умеет как есть:
            // переоткрываем, чтобы не пересчитывать звук.
            tracing::info!(from = self.device_rate, to = spec.sample_rate, "переоткрываю устройство на частоте файла");
            self.close_output();
        }
        self.ensure_output(Some(spec.sample_rate))?;

        if let Some(producer) = self.producer.as_mut() {
            producer.flush();
        }
        self.ended = false;

        let info = track_info(track, &source);

        let gain = self.replay_gain_factor(path);
        self.pipeline
            .load(source, gain, self.device_rate, self.device_channels)?;
        self.current_track = Some(track.clone());

        // Новый трек начинает отсчёт заново: кольцо сброшено, маркеров нет.
        self.frames_written = 0;
        self.markers.clear();
        self.current = Marker {
            start_frame: 0,
            track: Some(info.clone()),
            duration_ms: info.duration_ms.unwrap_or(0),
            segment_start_ms: 0,
        };

        self.state.set_track(Some(info.clone()));
        self.state.set_duration_ms(info.duration_ms.unwrap_or(0));
        self.state.set_position_ms(0);
        self.report_underruns();
        self.unprime();

        tracing::info!(path = %path.display(), codec = info.codec, "трек загружен");
        Ok(info)
    }

    /// Множитель ReplayGain для трека: метка + предусиление, с защитой от
    /// клиппинга по пику. Нет метки — ничего не трогаем, играем как есть.
    fn replay_gain_factor(&self, path: &Path) -> f32 {
        if self.replay_gain == ReplayGainMode::Off {
            return 1.0;
        }

        let marks = tags::read_replay_gain(path);
        let (gain_db, peak) = match self.replay_gain {
            ReplayGainMode::Album => (
                marks.album_gain_db.or(marks.track_gain_db),
                marks.album_peak.or(marks.track_peak),
            ),
            _ => (
                marks.track_gain_db.or(marks.album_gain_db),
                marks.track_peak.or(marks.album_peak),
            ),
        };

        let Some(gain_db) = gain_db else {
            return 1.0;
        };

        let mut factor = 10f32.powf((gain_db + self.replay_gain_preamp_db) / 20.0);
        if let Some(peak) = peak {
            // С известным пиком не выпускаем сигнал за 0 dBFS.
            factor = factor.min(1.0 / peak);
        }
        let factor = factor.clamp(0.0, 4.0);
        tracing::debug!(gain_db, factor, "ReplayGain применён");
        factor
    }

    /// Готовит следующий трек заранее — в этом весь gapless.
    fn set_next(&mut self, track: Option<TrackRef>) {
        let Some(track) = track else {
            self.pipeline.drop_next();
            self.next_info = None;
            return;
        };

        if !self.pipeline.has_source() {
            // Пока ничего не играет, готовить нечего: устройство ещё не открыто.
            return;
        }
        if !self.pipeline.accepts_next() {
            // Идёт кроссфейд: следующий трек уже звучит, подменять его поздно.
            return;
        }
        if self.next_track.as_ref() == Some(&track)
            || self.failed_next.as_ref() == Some(&track)
            || self.rate_skipped_next.as_ref() == Some(&track)
        {
            return;
        }

        let path = track.path.clone();
        match open_source(&track) {
            Ok(source) if self.needs_rate_switch(source.spec().sample_rate) => {
                tracing::debug!(path = %path.display(), "следующий трек другой частоты: откроем его заново");
                self.pipeline.drop_next();
                self.next_info = None;
                self.next_track = None;
                self.rate_skipped_next = Some(track);
            }
            Ok(source) => {
                let info = track_info(&track, &source);
                let gain = self.replay_gain_factor(&path);
                if let Err(err) = self.pipeline.queue_next(source, gain) {
                    tracing::warn!(%err, "следующий трек не встал в очередь");
                    self.next_info = None;
                    self.next_track = None;
                    return;
                }
                self.next_info = Some(info);
                self.next_track = Some(track);
                tracing::debug!(path = %path.display(), "следующий трек открыт заранее");
            }
            Err(err) => {
                tracing::warn!(%err, path = %path.display(), "следующий трек не открылся");
                self.pipeline.drop_next();
                self.next_info = None;
                self.next_track = None;
                self.failed_next = Some(track);
            }
        }
    }

    fn play(&mut self) {
        if !self.pipeline.has_source() {
            // После Stop источник закрыт: переоткрываем файл с начала.
            let Some(track) = self.current_track.clone() else {
                return;
            };
            self.load(&track, true);
            return;
        }
        self.set_playing(true);
    }

    fn seek(&mut self, position_ms: u64) {
        if !self.pipeline.has_source() {
            return;
        }

        // Не пускаем курсор ровно в конец: там декодеру нечего отдать и трек
        // просто мгновенно доиграет, что выглядит как проигнорированный клик.
        let duration = self.state.duration_ms();
        let target = if duration > 0 {
            position_ms.min(duration.saturating_sub(100))
        } else {
            position_ms
        };

        // Сначала глушим то, что уже лежит в кольце, иначе после прыжка
        // услышим ещё секунду старого места.
        if let Some(producer) = self.producer.as_mut() {
            producer.flush();
        }

        match self.pipeline.seek(target) {
            Ok(actual) => {
                // Кольцо сброшено: счётчики кадров и маркеры начинаются заново.
                self.frames_written = 0;
                self.markers.clear();
                self.current.start_frame = 0;
                self.current.segment_start_ms = actual;
                self.ended = false;
                self.unprime();
                self.state.set_position_ms(actual);
                tracing::debug!(requested = position_ms, actual, "перемотка");
            }
            Err(err) => self.report(&err),
        }
    }

    fn stop(&mut self) {
        self.set_playing(false);
        self.unprime();
        if let Some(producer) = self.producer.as_mut() {
            producer.flush();
        }
        self.pipeline.clear();
        self.frames_written = 0;
        self.markers.clear();
        self.current = Marker::default();
        self.next_info = None;
        self.next_track = None;
        self.failed_next = None;
        self.ended = false;
        self.state.set_position_ms(0);
        self.state.set_status(PlaybackStatus::Stopped);
        self.emit(EngineEvent::StatusChanged(PlaybackStatus::Stopped));
    }

    fn set_playing(&mut self, playing: bool) {
        if let Some(ring) = self.ring.as_ref() {
            ring.set_playing(playing);
        }
        let status = if playing {
            PlaybackStatus::Playing
        } else if self.pipeline.has_source() {
            PlaybackStatus::Paused
        } else {
            self.state.status()
        };
        if self.state.status() != status {
            tracing::debug!(?status, "смена статуса");
            self.state.set_status(status);
            self.emit(EngineEvent::StatusChanged(status));
        }
    }

    /// Открывает устройство, если поток ещё не создан.
    fn ensure_output(&mut self, preferred_rate: Option<u32>) -> Result<()> {
        if self.output.is_some() {
            return Ok(());
        }

        let target = output::select(self.device_id.as_deref(), preferred_rate)?;
        let rate = target.sample_rate();
        let channels = target.channels();

        let capacity = (self.buffer_ms as usize * rate as usize / 1000) * channels;
        let (producer, consumer) = ring::channel(capacity.max(MIN_RING_SAMPLES), self.state.volume());
        let shared = Arc::clone(producer.shared());
        // Новое устройство посреди затухания продолжает с того же места.
        shared.set_fade(self.fade);

        // Отвод живёт столько же, сколько поток устройства: новый поток —
        // новый отвод, а его потребитель уезжает анализатору.
        let (tap_producer, tap_consumer) =
            tap::channel(TAP_SAMPLES, channels, rate, Arc::clone(&self.analysis));
        if let Some(tap_tx) = self.tap_tx.as_ref() {
            let _ = tap_tx.send(tap_consumer);
        }

        let stream = output::open(target, consumer, tap_producer, self.error_tx.clone())?;
        stream.start()?;

        self.state.set_device(Some(stream.device_name.clone()));
        self.device_rate = rate;
        self.state.set_device_rate(rate);
        self.device_channels = channels;
        self.ring = Some(shared);
        // Счётчик пропусков у нового кольца свой, с нуля.
        self.underrun_mark = 0;
        self.producer = Some(producer);
        self.output = Some(stream);
        Ok(())
    }

    /// Стоит ли переоткрыть устройство под частоту `rate`: включён режим без
    /// пересчёта, частота другая, и устройство её умеет.
    fn needs_rate_switch(&self, rate: u32) -> bool {
        self.exact_rate
            && self.output.is_some()
            && self.device_rate != rate
            && output::supported_rates(self.device_id.as_deref()).contains(&rate)
    }

    /// Статус не трогаем: переоткрытие — это пауза на долю секунды внутри
    /// загрузки, а не «пауза» для интерфейса.
    fn close_output(&mut self) {
        if let Some(ring) = self.ring.as_ref() {
            ring.set_playing(false);
        }
        self.output = None;
        self.producer = None;
        self.ring = None;
        self.state.set_device_rate(0);
    }

    fn switch_device(&mut self, device_id: Option<String>) {
        self.device_id = device_id;
        let was_playing = self.state.status() == PlaybackStatus::Playing;
        let track = self.current_track.clone();

        self.set_playing(false);
        self.pipeline.clear();
        self.output = None;
        self.producer = None;
        self.ring = None;

        // Позиция пока теряется: перемотка появится на следующем шаге.
        if let Some(track) = track {
            self.load(&track, was_playing);
        }
    }

    fn update_position(&mut self) {
        let (Some(ring), true) = (self.ring.as_ref(), self.device_rate > 0) else {
            return;
        };
        let played = ring.frames_played();

        // Колбэк дошёл до границы трека — значит стык уже звучит.
        while self
            .markers
            .front()
            .is_some_and(|marker| played >= marker.start_frame)
        {
            let Some(marker) = self.markers.pop_front() else {
                break;
            };
            self.apply_marker(marker);
        }

        let elapsed = played.saturating_sub(self.current.start_frame);
        let mut position =
            self.current.segment_start_ms + elapsed * 1000 / u64::from(self.device_rate);
        if self.current.duration_ms > 0 {
            position = position.min(self.current.duration_ms);
        }
        self.state.set_position_ms(position);
    }

    /// Переключает «текущий трек» на тот, чья граница только что прозвучала.
    fn apply_marker(&mut self, marker: Marker) {
        if let Some(track) = marker.track.clone() {
            self.report_underruns();
            tracing::info!(title = track.title, "стык треков пройден без паузы");
            self.current_track = self.next_track.take();
            self.state.set_track(Some(track.clone()));
            self.state.set_duration_ms(track.duration_ms.unwrap_or(0));
            self.emit(EngineEvent::TrackChanged(track));
        }
        self.ended = false;
        self.current = marker;
    }

    fn check_finished(&mut self) {
        if self.ended || !self.pipeline.has_source() || !self.pipeline.is_drained() {
            return;
        }
        let drained = self
            .producer
            .as_ref()
            .map(|producer| producer.is_drained())
            .unwrap_or(true);
        if !drained {
            return;
        }

        self.ended = true;
        self.pipeline.clear();
        if let Some(ring) = self.ring.as_ref() {
            ring.set_playing(false);
            ring.set_expecting_data(false);
        }
        self.state.set_status(PlaybackStatus::Stopped);
        self.emit(EngineEvent::StatusChanged(PlaybackStatus::Stopped));
        self.emit(EngineEvent::Ended);

        self.report_underruns();
        tracing::debug!("трек доигран до конца");
    }

    /// Пустое кольцо до первой записи — тишина старта, а не пропуск.
    fn unprime(&mut self) {
        self.primed = false;
        if let Some(ring) = self.ring.as_ref() {
            ring.set_expecting_data(false);
        }
    }

    /// Пропуски за текущий трек: разница со счётчиком на его начале.
    fn report_underruns(&mut self) {
        let total = self.ring.as_ref().map(|ring| ring.underruns()).unwrap_or(0);
        let underruns = total.saturating_sub(self.underrun_mark);
        self.underrun_mark = total;
        if underruns > 0 {
            tracing::warn!(underruns, "за трек колбэк оставался без данных");
        }
    }

    fn drain_device_errors(&mut self) {
        while let Ok(message) = self.error_rx.try_recv() {
            tracing::error!(%message, "ошибка аудиопотока");
            let err = RingloftError::AudioDevice(message);
            self.report(&err);
            // Поток устройства мог развалиться — пересоздадим его при следующей загрузке.
            self.output = None;
            self.producer = None;
            self.ring = None;
        }
    }

    fn report(&self, err: &RingloftError) {
        tracing::error!(%err, "движок: ошибка");
        self.emit(EngineEvent::Failed(ErrorPayload::from(err)));
    }

    fn emit(&self, event: EngineEvent) {
        if self.events.send(event).is_err() {
            tracing::warn!("некому доставлять события движка");
        }
    }
}
