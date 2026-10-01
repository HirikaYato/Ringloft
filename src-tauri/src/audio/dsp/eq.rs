//! Эквалайзер: каскад пиковых биквад-фильтров (RBJ cookbook).
//!
//! Считается до кольца, а не в аудиоколлбэке. Причина простая: пересчёт
//! коэффициентов — это `sin`, `cos` и `powf`, а в RT-потоке им не место.
//! Цена — задержка отклика на размер кольца, зато колбэк остаётся стерильным.

use super::biquad::{Coefficients, Shape, State};

/// Центральные частоты полос, Гц (ISO, октавные).
pub const FREQUENCIES: [f32; 10] = [
    31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

/// Добротность для октавной полосы: Q = sqrt(2) даёт стык соседних полос
/// по уровню -3 дБ, ровно как ждут от графического эквалайзера.
const Q: f32 = std::f32::consts::SQRT_2;

/// На сколько децибел полоса подъезжает к цели за один блок обработки.
/// При блоках в ~20 мс перегон с нуля до 15 дБ занимает примерно полсекунды —
/// достаточно плавно, чтобы не щёлкало.
const GAIN_STEP_DB: f32 = 0.5;

pub struct Equalizer {
    sample_rate: f32,
    channels: usize,
    enabled: bool,
    /// Куда едем и где находимся сейчас — по каждой полосе и по предусилению.
    target_gains: [f32; FREQUENCIES.len()],
    current_gains: [f32; FREQUENCIES.len()],
    target_preamp_db: f32,
    current_preamp_db: f32,
    coefficients: [Coefficients; FREQUENCIES.len()],
    states: Vec<[State; FREQUENCIES.len()]>,
}

impl Equalizer {
    pub fn new(sample_rate: u32, channels: usize) -> Self {
        let mut eq = Self {
            sample_rate: sample_rate as f32,
            channels,
            enabled: false,
            target_gains: [0.0; FREQUENCIES.len()],
            current_gains: [0.0; FREQUENCIES.len()],
            target_preamp_db: 0.0,
            current_preamp_db: 0.0,
            coefficients: [Coefficients::default(); FREQUENCIES.len()],
            states: vec![[State::default(); FREQUENCIES.len()]; channels.max(1)],
        };
        eq.recompute();
        eq
    }

    pub fn configure(&mut self, enabled: bool, preamp_db: f32, bands_db: &[f32]) {
        self.enabled = enabled;
        self.target_preamp_db = if enabled { preamp_db.clamp(-15.0, 15.0) } else { 0.0 };
        for (index, target) in self.target_gains.iter_mut().enumerate() {
            *target = if enabled {
                bands_db.get(index).copied().unwrap_or(0.0).clamp(-15.0, 15.0)
            } else {
                0.0
            };
        }
    }

    /// Выключенный и уже «доехавший» до нуля эквалайзер ничего не делает —
    /// сигнал проходит мимо фильтров.
    fn is_idle(&self) -> bool {
        !self.enabled
            && self.current_preamp_db.abs() < 1e-4
            && self.current_gains.iter().all(|gain| gain.abs() < 1e-4)
    }

    /// Обрабатывает блок interleaved-сэмплов на месте.
    pub fn process(&mut self, samples: &mut [f32]) {
        if self.is_idle() {
            return;
        }
        self.advance_gains();

        let preamp = 10f32.powf(self.current_preamp_db / 20.0);
        let channels = self.channels.max(1);
        let last_channel = self.states.len() - 1;
        let coefficients = self.coefficients;

        for frame in samples.chunks_mut(channels) {
            for (channel, sample) in frame.iter_mut().enumerate() {
                let states = &mut self.states[channel.min(last_channel)];
                let mut value = *sample * preamp;
                for (band, state) in states.iter_mut().enumerate() {
                    value = state.step(value, &coefficients[band]);
                }
                *sample = value;
            }
        }
    }

    /// Подтягивает текущие усиления к целевым и пересчитывает коэффициенты.
    fn advance_gains(&mut self) {
        let mut changed = false;
        for (current, target) in self.current_gains.iter_mut().zip(self.target_gains) {
            if (*current - target).abs() > 1e-4 {
                let step = (target - *current).clamp(-GAIN_STEP_DB, GAIN_STEP_DB);
                *current += step;
                changed = true;
            }
        }
        if (self.current_preamp_db - self.target_preamp_db).abs() > 1e-4 {
            let step =
                (self.target_preamp_db - self.current_preamp_db).clamp(-GAIN_STEP_DB, GAIN_STEP_DB);
            self.current_preamp_db += step;
        }
        if changed {
            self.recompute();
        }
    }

    fn recompute(&mut self) {
        for (index, frequency) in FREQUENCIES.iter().enumerate() {
            self.coefficients[index] = Coefficients::new(
                Shape::Peak,
                *frequency,
                self.current_gains[index],
                Q,
                self.sample_rate,
            );
        }
    }

    /// Сбрасывает состояние фильтров — после перемотки и смены трека.
    pub fn reset(&mut self) {
        for states in &mut self.states {
            *states = [State::default(); FREQUENCIES.len()];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn sine(frequency: f32, rate: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|index| (2.0 * PI * frequency * index as f32 / rate).sin() * 0.5)
            .collect()
    }

    /// Уровень одной частоты в сигнале (алгоритм Гёрцеля).
    fn level_at(samples: &[f32], frequency: f32, rate: f32) -> f32 {
        let omega = 2.0 * PI * frequency / rate;
        let coefficient = 2.0 * omega.cos();
        let (mut s1, mut s2) = (0.0_f32, 0.0_f32);
        for sample in samples {
            let s0 = sample + coefficient * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        (s1 * s1 + s2 * s2 - coefficient * s1 * s2).max(0.0).sqrt() / samples.len() as f32 * 2.0
    }

    fn settle(eq: &mut Equalizer, rate: f32, frequency: f32) -> Vec<f32> {
        // Прогоняем достаточно блоков, чтобы усиления доехали до цели.
        for _ in 0..64 {
            let mut warmup = sine(frequency, rate, 1024);
            eq.process(&mut warmup);
        }
        let mut block = sine(frequency, rate, 8192);
        eq.process(&mut block);
        block
    }

    #[test]
    fn flat_equalizer_does_not_touch_the_signal() {
        let rate = 48_000.0;
        let mut eq = Equalizer::new(48_000, 1);
        eq.configure(true, 0.0, &[0.0; 10]);

        let processed = settle(&mut eq, rate, 1000.0);
        let reference = sine(1000.0, rate, 8192);
        let difference = 20.0
            * (level_at(&processed, 1000.0, rate) / level_at(&reference, 1000.0, rate)).log10();
        assert!(
            difference.abs() < 0.1,
            "ровный эквалайзер изменил уровень на {difference:.2} дБ"
        );
    }

    #[test]
    fn boosts_only_its_own_band() {
        let rate = 48_000.0;
        let mut bands = [0.0_f32; 10];
        bands[5] = 6.0; // 1000 Гц

        let mut eq = Equalizer::new(48_000, 1);
        eq.configure(true, 0.0, &bands);

        let boosted = settle(&mut eq, rate, 1000.0);
        let reference = sine(1000.0, rate, 8192);
        let gain = 20.0
            * (level_at(&boosted, 1000.0, rate) / level_at(&reference, 1000.0, rate)).log10();
        assert!(
            (gain - 6.0).abs() < 0.5,
            "на своей частоте ожидали +6 дБ, получили {gain:.2}"
        );

        // Дальняя частота почти не должна шевельнуться.
        let mut eq = Equalizer::new(48_000, 1);
        eq.configure(true, 0.0, &bands);
        let far = settle(&mut eq, rate, 60.0);
        let far_reference = sine(60.0, rate, 8192);
        let far_gain =
            20.0 * (level_at(&far, 60.0, rate) / level_at(&far_reference, 60.0, rate)).log10();
        assert!(
            far_gain.abs() < 1.0,
            "на 60 Гц полоса 1 кГц не должна давать {far_gain:.2} дБ"
        );
    }

    #[test]
    fn disabled_equalizer_is_a_wire() {
        let mut eq = Equalizer::new(48_000, 2);
        eq.configure(false, 12.0, &[12.0; 10]);
        let mut samples = vec![0.25, -0.25, 0.5, -0.5];
        let original = samples.clone();
        eq.process(&mut samples);
        assert_eq!(samples, original);
    }
}
