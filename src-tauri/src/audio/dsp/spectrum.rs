//! Анализатор спектра: окно Ханна, вещественное БПФ, логарифмические полосы.

use std::sync::Arc;

use realfft::RealFftPlanner;
use realfft::num_complex::Complex;

/// Размер окна БПФ. 2048 на 48 кГц — это 43 мс и шаг по частоте 23 Гц:
/// достаточно и для баса, и чтобы картинка не запаздывала.
pub const FFT_SIZE: usize = 2048;
/// Нижняя и верхняя границы картинки.
const MIN_HZ: f32 = 40.0;
const MAX_HZ: f32 = 16_000.0;
/// Динамический диапазон полосок.
const FLOOR_DB: f32 = -72.0;
/// Насколько быстро полоска падает. Подъём мгновенный — так пики видны.
const DECAY: f32 = 0.82;

pub struct Analyzer {
    fft: Arc<dyn realfft::RealToComplex<f32>>,
    window: Vec<f32>,
    /// Кольцо из последних FFT_SIZE моно-сэмплов и позиция записи.
    history: Vec<f32>,
    write: usize,
    scratch: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    /// Границы полос в отсчётах БПФ.
    bins: Vec<(usize, usize)>,
    levels: Vec<f32>,
}

impl Analyzer {
    pub fn new(sample_rate: u32, bands: usize) -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
        let spectrum = fft.make_output_vec();

        // Окно Ханна: без него у синуса «растекаются» соседние полосы.
        let window = (0..FFT_SIZE)
            .map(|index| {
                let phase =
                    2.0 * std::f32::consts::PI * index as f32 / (FFT_SIZE as f32 - 1.0);
                0.5 - 0.5 * phase.cos()
            })
            .collect();

        Self {
            fft,
            window,
            history: vec![0.0; FFT_SIZE],
            write: 0,
            scratch: vec![0.0; FFT_SIZE],
            spectrum,
            bins: band_bins(sample_rate, bands),
            levels: vec![0.0; bands],
        }
    }

    /// Добавляет свежий interleaved-звук, сводя его в моно.
    pub fn feed(&mut self, samples: &[f32], channels: usize) {
        if channels == 0 || samples.is_empty() {
            return;
        }
        let scale = 1.0 / channels as f32;
        for frame in samples.chunks_exact(channels) {
            // Кольцевая запись: сдвигать буфер на каждый сэмпл — это
            // сотня миллионов операций в секунду на ровном месте.
            self.history[self.write] = frame.iter().sum::<f32>() * scale;
            self.write = (self.write + 1) % FFT_SIZE;
        }
    }

    /// Считает полосы по накопленному окну. Значения 0..1.
    pub fn compute(&mut self) -> &[f32] {
        // Разворачиваем кольцо в хронологический порядок и сразу взвешиваем окном.
        for index in 0..FFT_SIZE {
            let source = (self.write + index) % FFT_SIZE;
            self.scratch[index] = self.history[source] * self.window[index];
        }

        if self.fft.process(&mut self.scratch, &mut self.spectrum).is_err() {
            return &self.levels;
        }

        let normalize = 2.0 / FFT_SIZE as f32;
        for (band, (from, to)) in self.bins.iter().enumerate() {
            let mut peak = 0.0_f32;
            for bin in *from..=*to {
                if let Some(value) = self.spectrum.get(bin) {
                    peak = peak.max(value.norm() * normalize);
                }
            }

            let db = 20.0 * (peak + 1e-9).log10();
            let level = ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0);
            // Вверх — сразу, вниз — плавно: так глаз видит удары, а не мельтешение.
            self.levels[band] = level.max(self.levels[band] * DECAY);
        }

        &self.levels
    }

    pub fn reset(&mut self) {
        self.history.fill(0.0);
        self.write = 0;
        self.levels.fill(0.0);
    }
}

/// Логарифмическая сетка полос: низам достаётся столько же места, сколько верхам.
fn band_bins(sample_rate: u32, bands: usize) -> Vec<(usize, usize)> {
    let nyquist = sample_rate as f32 / 2.0;
    let bin_width = nyquist / (FFT_SIZE / 2) as f32;
    let ratio = (MAX_HZ / MIN_HZ).powf(1.0 / bands as f32);

    let mut bins = Vec::with_capacity(bands);
    let mut low = MIN_HZ;
    for _ in 0..bands {
        let high = low * ratio;
        let from = ((low / bin_width) as usize).max(1);
        let to = ((high / bin_width) as usize).max(from).min(FFT_SIZE / 2);
        bins.push((from, to));
        low = high;
    }
    bins
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_band_of_a_pure_tone() {
        let rate = 48_000;
        let bands = 48;
        let mut analyzer = Analyzer::new(rate, bands);

        // Синус 1 кГц, моно.
        let samples: Vec<f32> = (0..FFT_SIZE * 2)
            .map(|index| {
                (2.0 * std::f32::consts::PI * 1000.0 * index as f32 / rate as f32).sin() * 0.5
            })
            .collect();
        analyzer.feed(&samples, 1);
        let levels = analyzer.compute().to_vec();

        let loudest = levels
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(index, _)| index)
            .expect("полосы");

        // Полоса, в которую попадает 1000 Гц при логарифмической сетке.
        let ratio: f32 = (MAX_HZ / MIN_HZ).powf(1.0 / bands as f32);
        let expected = ((1000.0_f32 / MIN_HZ).log10() / ratio.log10()) as usize;
        assert!(
            loudest.abs_diff(expected) <= 1,
            "тон 1 кГц оказался в полосе {loudest}, ожидали около {expected}"
        );
        assert!(levels[loudest] > 0.5, "уровень слишком мал: {}", levels[loudest]);
    }

    #[test]
    fn silence_gives_zero() {
        let mut analyzer = Analyzer::new(48_000, 32);
        analyzer.feed(&vec![0.0; FFT_SIZE * 2], 1);
        assert!(analyzer.compute().iter().all(|value| *value < 0.01));
    }
}
