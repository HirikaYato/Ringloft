//! Конвейер: текущий трек, следующий и переход между ними.
//!
//! Два режима стыка. Без кроссфейда — gapless: следующий трек вступает ровно
//! там, где кончился предыдущий. С кроссфейдом — оба трека какое-то время
//! звучат одновременно, с равномощным затуханием одного и нарастанием другого.

use std::f32::consts::FRAC_PI_2;

use super::dsp::eq::Equalizer;
use super::dsp::peq::ParametricEq;
use crate::settings::HeadphoneProfile;
use super::ring::RingProducer;
use super::stream::Stream;
use crate::error::{RingloftError, Result};

/// Сколько сэмплов готовим за один заход. ~20 мс стерео на 48 кГц.
const MAX_BLOCK: usize = 2048;

pub(super) enum PumpOutcome {
    /// Что-то записали в кольцо — можно продолжать без паузы.
    Wrote {
        samples: usize,
        /// Эти сэмплы — уже следующий трек: движку пора поставить маркер.
        started_next: bool,
    },
    /// Делать нечего: кольцо заполнено или трек не загружен.
    Idle,
    /// Всё отдано в кольцо, продолжения нет.
    Drained,
    Failed(RingloftError),
}

/// Настройки эквалайзера в том виде, в каком их носит движок.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct EqSettings {
    pub enabled: bool,
    pub preamp_db: f32,
    pub bands_db: Vec<f32>,
}

impl Default for EqSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            preamp_db: 0.0,
            bands_db: vec![0.0; super::dsp::eq::FREQUENCIES.len()],
        }
    }
}

pub(super) struct Pipeline {
    current: Option<Stream>,
    next: Option<Stream>,
    device_rate: u32,
    out_channels: usize,
    /// Эквалайзер работает на частоте устройства и переживает смену трека:
    /// пересоздавать его на стыке — значит сбросить состояние фильтров.
    eq: Option<Equalizer>,
    eq_settings: EqSettings,
    /// Профиль наушников (AutoEQ) — после графического эквалайзера.
    peq: Option<ParametricEq>,
    headphone: Option<HeadphoneProfile>,
    crossfade_ms: u32,
    fading: bool,
    fade_done: u64,
    /// Буфер микса и то, что не влезло в кольцо с прошлого раза.
    out: Vec<f32>,
    pending: Vec<f32>,
    pending_pos: usize,
}

impl Pipeline {
    pub fn new() -> Self {
        Self {
            current: None,
            next: None,
            device_rate: 0,
            out_channels: 0,
            eq: None,
            eq_settings: EqSettings::default(),
            peq: None,
            headphone: None,
            crossfade_ms: 0,
            fading: false,
            fade_done: 0,
            out: Vec::new(),
            pending: Vec::new(),
            pending_pos: 0,
        }
    }

    pub fn clear(&mut self) {
        self.current = None;
        self.next = None;
        self.fading = false;
        self.fade_done = 0;
        self.out.clear();
        self.pending.clear();
        self.pending_pos = 0;
    }

    pub fn load(
        &mut self,
        source: super::decoder::TrackSource,
        gain: f32,
        device_rate: u32,
        device_channels: usize,
    ) -> Result<()> {
        self.clear();
        self.device_rate = device_rate;
        self.out_channels = device_channels;

        let mut equalizer = Equalizer::new(device_rate, device_channels);
        equalizer.configure(
            self.eq_settings.enabled,
            self.eq_settings.preamp_db,
            &self.eq_settings.bands_db,
        );
        self.eq = Some(equalizer);
        self.peq = self
            .headphone
            .as_ref()
            .map(|profile| ParametricEq::new(profile, device_rate, device_channels));

        self.current = Some(Stream::new(source, gain, device_rate, device_channels)?);
        Ok(())
    }

    /// Во время перехода очередь трогать нельзя: там уже звучит следующий трек.
    /// Название трека у текущего источника — есть только у радио.
    pub fn stream_title(&self) -> Option<String> {
        self.current.as_ref().and_then(|stream| stream.stream_title())
    }

    pub fn accepts_next(&self) -> bool {
        !self.fading && self.device_rate > 0
    }

    /// Кладёт трек «на очередь»: он вступит в дело сам, без команды снаружи.
    pub fn queue_next(&mut self, source: super::decoder::TrackSource, gain: f32) -> Result<()> {
        if !self.accepts_next() {
            return Ok(());
        }
        self.next = Some(Stream::new(
            source,
            gain,
            self.device_rate,
            self.out_channels,
        )?);
        Ok(())
    }

    pub fn drop_next(&mut self) {
        if !self.fading {
            self.next = None;
        }
    }

    pub fn has_source(&self) -> bool {
        self.current.is_some()
    }

    /// Всё отдано в кольцо и продолжения нет.
    pub fn is_drained(&self) -> bool {
        self.pending_pos >= self.pending.len()
            && self.next.is_none()
            && self.current.as_ref().is_none_or(|stream| stream.is_drained())
    }

    pub fn set_gain(&mut self, gain: f32) {
        if let Some(stream) = self.current.as_mut() {
            stream.set_gain(gain);
        }
    }

    pub fn set_eq(&mut self, settings: EqSettings) {
        if let Some(eq) = self.eq.as_mut() {
            eq.configure(settings.enabled, settings.preamp_db, &settings.bands_db);
        }
        self.eq_settings = settings;
    }

    /// Профиль наушников; `None` — выключен. Применяется сразу, если
    /// устройство уже открыто.
    pub fn set_headphone(&mut self, profile: Option<HeadphoneProfile>) {
        self.peq = match (&profile, self.device_rate) {
            (Some(profile), rate) if rate > 0 => {
                Some(ParametricEq::new(profile, rate, self.out_channels))
            }
            _ => None,
        };
        self.headphone = profile;
    }

    pub fn set_crossfade(&mut self, crossfade_ms: u32) {
        self.crossfade_ms = crossfade_ms.min(10_000);
    }

    pub fn seek(&mut self, position_ms: u64) -> Result<u64> {
        let Some(stream) = self.current.as_mut() else {
            return Err(RingloftError::Internal("перемотка без загруженного трека".into()));
        };
        let actual = stream.seek(position_ms)?;

        // Переход отменяется: мы больше не в конце трека.
        self.fading = false;
        self.fade_done = 0;
        self.out.clear();
        self.pending.clear();
        self.pending_pos = 0;
        if let Some(eq) = self.eq.as_mut() {
            eq.reset();
        }
        if let Some(peq) = self.peq.as_mut() {
            peq.reset();
        }
        Ok(actual)
    }

    fn fade_frames(&self) -> u64 {
        u64::from(self.crossfade_ms) * u64::from(self.device_rate) / 1000
    }

    pub fn pump(&mut self, producer: &mut RingProducer) -> PumpOutcome {
        let free = producer.free();
        if free == 0 {
            return PumpOutcome::Idle;
        }

        // Сначала дописываем то, что не влезло в прошлый раз.
        if self.pending_pos < self.pending.len() {
            let written = producer.write(&self.pending[self.pending_pos..]);
            self.pending_pos += written;
            if self.pending_pos >= self.pending.len() {
                self.pending.clear();
                self.pending_pos = 0;
            }
            return PumpOutcome::Wrote {
                samples: written,
                started_next: false,
            };
        }

        let channels = self.out_channels.max(1);
        let wanted = free.min(MAX_BLOCK) / channels * channels;
        if wanted == 0 || self.current.is_none() {
            return PumpOutcome::Idle;
        }

        let fade_frames = self.fade_frames();
        let mut started_next = false;

        // Пора ли начинать переход? Решаем до того, как займём буферы.
        if !self.fading && fade_frames > 0 && self.next.is_some() {
            let close_to_end = self
                .current
                .as_ref()
                .and_then(|stream| stream.frames_left())
                .is_some_and(|left| left <= fade_frames);
            if close_to_end {
                self.fading = true;
                self.fade_done = 0;
                started_next = true;
                tracing::debug!(crossfade_ms = self.crossfade_ms, "начинаю кроссфейд");
            }
        }

        self.out.clear();
        let result = if self.fading {
            self.pump_crossfade(wanted, fade_frames, channels)
        } else {
            self.pump_single(wanted, &mut started_next)
        };
        if let Err(err) = result {
            return PumpOutcome::Failed(err);
        }

        if self.out.is_empty() {
            return if self.is_drained() {
                PumpOutcome::Drained
            } else {
                PumpOutcome::Idle
            };
        }

        if let Some(eq) = self.eq.as_mut() {
            eq.process(&mut self.out);
        }
        if let Some(peq) = self.peq.as_mut() {
            peq.process(&mut self.out);
        }

        let written = producer.write(&self.out);
        if written < self.out.len() {
            self.pending.clear();
            self.pending.extend_from_slice(&self.out[written..]);
            self.pending_pos = 0;
        }

        PumpOutcome::Wrote {
            samples: written,
            started_next,
        }
    }

    /// Обычная работа: один трек, при необходимости — бесшовный переход.
    fn pump_single(&mut self, wanted: usize, started_next: &mut bool) -> Result<()> {
        {
            let Some(stream) = self.current.as_mut() else {
                return Ok(());
            };
            stream.fill(wanted)?;
            if !stream.is_drained() {
                let block = stream.take(wanted);
                self.out.extend_from_slice(block);
                return Ok(());
            }
        }

        // Текущий трек иссяк — вступает заранее открытый следующий.
        let Some(next) = self.next.take() else {
            return Ok(());
        };
        self.current = Some(next);
        *started_next = true;

        let Some(stream) = self.current.as_mut() else {
            return Ok(());
        };
        stream.fill(wanted)?;
        let block = stream.take(wanted);
        self.out.extend_from_slice(block);
        Ok(())
    }

    /// Перекрытие: старый трек затухает, новый нарастает.
    fn pump_crossfade(&mut self, wanted: usize, fade_frames: u64, channels: usize) -> Result<()> {
        {
            let (Some(current), Some(next)) = (self.current.as_mut(), self.next.as_mut()) else {
                self.fading = false;
                return Ok(());
            };
            current.fill(wanted)?;
            next.fill(wanted)?;

            let old = current.take(wanted);
            let fresh = next.take(wanted);
            let frames = old.len().max(fresh.len()) / channels;

            for frame in 0..frames {
                // Равномощная кривая: сумма квадратов усилений постоянна,
                // поэтому на стыке не проваливается громкость.
                let position = (self.fade_done + frame as u64) as f32 / fade_frames.max(1) as f32;
                let angle = position.clamp(0.0, 1.0) * FRAC_PI_2;
                let (fade_out, fade_in) = (angle.cos(), angle.sin());

                for channel in 0..channels {
                    let index = frame * channels + channel;
                    let a = old.get(index).copied().unwrap_or(0.0);
                    let b = fresh.get(index).copied().unwrap_or(0.0);
                    self.out.push(a * fade_out + b * fade_in);
                }
            }
            self.fade_done += frames as u64;
        }

        let old_done = self
            .current
            .as_ref()
            .is_some_and(|stream| stream.is_drained());
        if self.fade_done >= fade_frames || old_done {
            self.current = self.next.take();
            self.fading = false;
            self.fade_done = 0;
            tracing::debug!("кроссфейд закончен");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::decoder::TrackSource;
    use crate::audio::ring;
    use crate::audio::test_wav;

    fn collect(pipeline: &mut Pipeline, frames_expected: usize, channels: usize) -> Vec<f32> {
        let (mut producer, mut consumer) = ring::channel(frames_expected * channels * 4, 1.0);
        let mut collected = Vec::new();
        let mut buffer = vec![0.0_f32; 8192];

        for _ in 0..20_000 {
            match pipeline.pump(&mut producer) {
                PumpOutcome::Wrote { .. } | PumpOutcome::Idle => {}
                PumpOutcome::Drained => break,
                PumpOutcome::Failed(err) => panic!("конвейер сломался: {err}"),
            }
            let taken = consumer.read(&mut buffer);
            collected.extend_from_slice(&buffer[..taken]);
        }
        let taken = consumer.read(&mut buffer);
        collected.extend_from_slice(&buffer[..taken]);
        collected
    }

    /// Главная проверка gapless: между треками не должно появиться ни одного
    /// лишнего сэмпла — ни тишины, ни обрыва.
    #[test]
    fn switches_to_next_track_without_a_gap() {
        let rate = 44_100;
        let channels = 2;
        let frames = 1000;

        let first = test_wav::temp_path("gapless-a");
        let second = test_wav::temp_path("gapless-b");
        test_wav::constant(&first, rate, channels as u16, frames, 0.5);
        test_wav::constant(&second, rate, channels as u16, frames, -0.5);

        let mut pipeline = Pipeline::new();
        pipeline
            .load(
                TrackSource::open(&first).expect("первый трек"),
                1.0,
                rate,
                channels,
            )
            .expect("загрузка");
        pipeline
            .queue_next(TrackSource::open(&second).expect("второй трек"), 1.0)
            .expect("очередь");

        let collected = collect(&mut pipeline, frames * 2, channels);

        assert_eq!(
            collected.len(),
            frames * channels * 2,
            "должны прозвучать оба трека целиком"
        );
        let boundary = frames * channels;
        assert!(collected[..boundary].iter().all(|value| *value > 0.4));
        assert!(collected[boundary..].iter().all(|value| *value < -0.4));

        let _ = std::fs::remove_file(&first);
        let _ = std::fs::remove_file(&second);
    }

    /// При кроссфейде треки перекрываются, а суммарная громкость не проваливается.
    #[test]
    fn crossfade_overlaps_tracks() {
        let rate = 8000;
        let channels = 1;
        let frames = 8000; // одна секунда

        let first = test_wav::temp_path("fade-a");
        let second = test_wav::temp_path("fade-b");
        test_wav::constant(&first, rate, 1, frames, 0.5);
        test_wav::constant(&second, rate, 1, frames, 0.5);

        let mut pipeline = Pipeline::new();
        pipeline.set_crossfade(500); // полсекунды
        pipeline
            .load(
                TrackSource::open(&first).expect("первый трек"),
                1.0,
                rate,
                channels,
            )
            .expect("загрузка");
        pipeline
            .queue_next(TrackSource::open(&second).expect("второй трек"), 1.0)
            .expect("очередь");

        let collected = collect(&mut pipeline, frames * 2, channels);

        // Перекрытие укорачивает общую длину примерно на длину перехода.
        let total = collected.len();
        assert!(
            total < frames * 2 - 1000 && total > frames,
            "ожидали перекрытие, получили {total} кадров"
        );

        // В середине перехода уровень держится: равномощная кривая даёт
        // 0.5*cos + 0.5*sin >= 0.5 при любом положении.
        let fade_start = frames - 4000;
        let during = &collected[fade_start..(fade_start + 3500).min(total)];
        let minimum = during.iter().cloned().fold(f32::MAX, f32::min);
        assert!(
            minimum > 0.45,
            "на переходе громкость просела до {minimum:.3}"
        );

        let _ = std::fs::remove_file(&first);
        let _ = std::fs::remove_file(&second);
    }
}
