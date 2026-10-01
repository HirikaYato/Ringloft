//! Один декодируемый трек со всей своей обвязкой: каналы, ресемплер, уровень.
//!
//! Отдельная структура нужна кроссфейду: во время перехода два трека звучат
//! одновременно, и у каждого своя частота, свои каналы и своя метка громкости.

use super::decoder::TrackSource;
use super::mix::map_channels;
use super::resample::Resampler;
use crate::error::Result;

pub(super) struct Stream {
    source: TrackSource,
    resampler: Option<Resampler>,
    in_channels: usize,
    out_channels: usize,
    device_rate: u32,
    /// Множитель ReplayGain этого трека.
    gain: f32,
    decoded: Vec<f32>,
    staged: Vec<f32>,
    /// Готовые сэмплы на частоте устройства.
    ready: Vec<f32>,
    ready_pos: usize,
    finished: bool,
    /// Сколько кадров уже отдано наружу — по нему считаем, далеко ли до конца.
    produced_frames: u64,
}

impl Stream {
    pub fn new(
        source: TrackSource,
        gain: f32,
        device_rate: u32,
        out_channels: usize,
    ) -> Result<Self> {
        let spec = source.spec();
        let resampler = if spec.sample_rate == device_rate {
            None
        } else {
            tracing::debug!(from = spec.sample_rate, to = device_rate, "включаю ресемплинг");
            Some(Resampler::new(spec.sample_rate, device_rate, out_channels)?)
        };

        Ok(Self {
            source,
            resampler,
            in_channels: spec.channels,
            out_channels,
            device_rate,
            gain,
            decoded: Vec::new(),
            staged: Vec::new(),
            ready: Vec::new(),
            ready_pos: 0,
            finished: false,
            produced_frames: 0,
        })
    }

    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain;
    }

    /// Кончился ли трек: источник иссяк и готовые сэмплы разобраны.
    pub fn is_drained(&self) -> bool {
        self.finished && self.ready_pos >= self.ready.len()
    }

    /// Сколько кадров осталось до конца трека. `None` — длительность неизвестна.
    /// Название трека от радиостанции: у файлов его нет.
    pub fn stream_title(&self) -> Option<String> {
        self.source.stream_title()
    }

    pub fn frames_left(&self) -> Option<u64> {
        let duration_ms = self.source.duration_ms()?;
        let total = duration_ms * u64::from(self.device_rate) / 1000;
        Some(total.saturating_sub(self.produced_frames))
    }

    pub fn seek(&mut self, position_ms: u64) -> Result<u64> {
        let actual = self.source.seek(position_ms)?;
        self.decoded.clear();
        self.staged.clear();
        self.ready.clear();
        self.ready_pos = 0;
        self.finished = false;
        self.produced_frames = actual * u64::from(self.device_rate) / 1000;
        if let Some(resampler) = self.resampler.as_mut() {
            resampler.reset();
        }
        Ok(actual)
    }

    /// Готовит хотя бы `wanted` сэмплов (или сколько получится, если трек кончился).
    pub fn fill(&mut self, wanted: usize) -> Result<()> {
        while !self.finished && self.available() < wanted {
            self.compact();

            if !self.source.next_block(&mut self.decoded)? {
                self.finished = true;
                if let Some(resampler) = self.resampler.as_mut() {
                    resampler.finish(&mut self.ready)?;
                }
                break;
            }

            self.staged.clear();
            map_channels(
                &self.decoded,
                self.in_channels,
                self.out_channels,
                &mut self.staged,
            );

            if (self.gain - 1.0).abs() > 1e-4 {
                for sample in &mut self.staged {
                    *sample *= self.gain;
                }
            }

            match self.resampler.as_mut() {
                Some(resampler) => resampler.push(&self.staged, &mut self.ready)?,
                None => self.ready.extend_from_slice(&self.staged),
            }
        }
        Ok(())
    }

    pub fn available(&self) -> usize {
        self.ready.len() - self.ready_pos
    }

    /// Отдаёт до `wanted` сэмплов и считает их отданными.
    pub fn take(&mut self, wanted: usize) -> &[f32] {
        let end = (self.ready_pos + wanted).min(self.ready.len());
        let slice = &self.ready[self.ready_pos..end];
        self.ready_pos = end;
        self.produced_frames += (slice.len() / self.out_channels.max(1)) as u64;
        slice
    }

    /// Выбрасывает уже отданное, чтобы буфер не рос бесконечно.
    fn compact(&mut self) {
        if self.ready_pos == 0 {
            return;
        }
        self.ready.drain(..self.ready_pos);
        self.ready_pos = 0;
    }
}
