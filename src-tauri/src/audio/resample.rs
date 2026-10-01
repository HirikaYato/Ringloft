//! Ресемплинг rubato, когда частота файла не совпадает с частотой устройства.
//!
//! Берём синхронный FFT-ресемплер: отношение частот фиксировано (44100 → 48000
//! это ровно 147/160), качество высокое, стоимость предсказуемая. Работаем
//! прямо с interleaved-данными через адаптер, без раскладки по каналам.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Indexing, Resampler as RubatoResampler};

use crate::error::{RingloftError, Result};

/// Размер входного чанка в фреймах. ~23 мс при 44.1 кГц — компромисс между
/// накладными расходами FFT и задержкой реакции на команды.
const CHUNK_FRAMES: usize = 1024;

pub struct Resampler {
    inner: Fft<f32>,
    channels: usize,
    chunk_frames: usize,
    /// Вход, не набравший полный чанк.
    pending: Vec<f32>,
    out_scratch: Vec<f32>,
}

impl Resampler {
    pub fn new(input_rate: u32, output_rate: u32, channels: usize) -> Result<Self> {
        let inner = Fft::<f32>::new(
            input_rate as usize,
            output_rate as usize,
            CHUNK_FRAMES,
            channels,
            FixedSync::Input,
        )
        .map_err(|err| RingloftError::Internal(format!("ресемплер: {err}")))?;

        let chunk_frames = inner.input_frames_next();
        let out_scratch = vec![0.0; inner.output_frames_max() * channels];

        Ok(Self {
            inner,
            channels,
            chunk_frames,
            pending: Vec::with_capacity(chunk_frames * channels * 2),
            out_scratch,
        })
    }

    /// Добавляет вход и дописывает готовые сэмплы в `out`.
    pub fn push(&mut self, input: &[f32], out: &mut Vec<f32>) -> Result<()> {
        self.pending.extend_from_slice(input);
        while self.pending.len() >= self.chunk_frames * self.channels {
            self.process_chunk(self.chunk_frames, out)?;
        }
        Ok(())
    }

    /// Сбрасывает состояние фильтров — после перемотки старый хвост не нужен.
    pub fn reset(&mut self) {
        self.inner.reset();
        self.pending.clear();
    }

    /// Досчитывает остаток в конце трека, дополняя его тишиной.
    pub fn finish(&mut self, out: &mut Vec<f32>) -> Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let valid = self.pending.len() / self.channels;
        self.pending.resize(self.chunk_frames * self.channels, 0.0);
        self.process_chunk(valid, out)
    }

    /// `valid_frames` меньше чанка только в самом конце трека.
    fn process_chunk(&mut self, valid_frames: usize, out: &mut Vec<f32>) -> Result<()> {
        let frames = self.chunk_frames;
        let channels = self.channels;

        let input = InterleavedSlice::new(&self.pending[..frames * channels], channels, frames)
            .map_err(|err| RingloftError::Internal(format!("ресемплер, вход: {err}")))?;

        let max_out = self.out_scratch.len() / channels;
        let mut output = InterleavedSlice::new_mut(&mut self.out_scratch, channels, max_out)
            .map_err(|err| RingloftError::Internal(format!("ресемплер, выход: {err}")))?;

        let indexing = (valid_frames < frames).then(|| Indexing::new().partial_len(valid_frames));

        let (read, written) = self
            .inner
            .process_into_buffer(&input, &mut output, indexing.as_ref())
            .map_err(|err| RingloftError::Internal(format!("ресемплер: {err}")))?;

        out.extend_from_slice(&self.out_scratch[..written * channels]);
        self.pending.drain(..read.min(frames) * channels);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Секунда стерео 44.1 кГц должна превратиться примерно в секунду 48 кГц.
    /// Допуск — один чанк: ресемплер отдаёт данные порциями и держит хвост.
    #[test]
    fn upsamples_44100_to_48000() {
        let channels = 2;
        let mut resampler = Resampler::new(44_100, 48_000, channels).expect("создание ресемплера");

        let input = vec![0.25_f32; 44_100 * channels];
        let mut out = Vec::new();
        resampler.push(&input, &mut out).expect("push");
        resampler.finish(&mut out).expect("finish");

        let frames = out.len() / channels;
        let expected = 48_000_isize;
        let diff = (frames as isize - expected).abs();
        assert!(
            diff < CHUNK_FRAMES as isize,
            "получили {frames} фреймов вместо ~{expected}"
        );
    }

    #[test]
    fn same_rate_is_not_resampled_here() {
        // Движок не создаёт ресемплер, когда частоты совпадают, но сама
        // конструкция с равными частотами должна работать без сюрпризов.
        let mut resampler = Resampler::new(48_000, 48_000, 2).expect("создание ресемплера");
        let mut out = Vec::new();
        resampler.push(&vec![0.0; 4096], &mut out).expect("push");
        assert!(out.len() <= 4096);
    }
}
