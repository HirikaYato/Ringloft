//! Параметрический эквалайзер — профиль наушников (AutoEQ). Стоит после
//! графического, считается в потоке движка, как и он.
//!
//! Профиль меняют редко (выбрали наушники — и всё), поэтому плавного
//! перехода коэффициентов, как у ползунков графического, здесь нет: фильтры
//! просто пересобираются.

use super::biquad::{Coefficients, Shape, State};
use crate::settings::{FilterShape, HeadphoneProfile};

pub struct ParametricEq {
    channels: usize,
    preamp: f32,
    coefficients: Vec<Coefficients>,
    /// По набору состояний на канал.
    states: Vec<Vec<State>>,
}

impl ParametricEq {
    pub fn new(profile: &HeadphoneProfile, sample_rate: u32, channels: usize) -> Self {
        let channels = channels.max(1);
        let coefficients: Vec<Coefficients> = profile
            .filters
            .iter()
            .map(|filter| {
                let shape = match filter.shape {
                    FilterShape::Peak => Shape::Peak,
                    FilterShape::LowShelf => Shape::LowShelf,
                    FilterShape::HighShelf => Shape::HighShelf,
                };
                Coefficients::new(shape, filter.frequency, filter.gain_db, filter.q, sample_rate as f32)
            })
            .collect();
        Self {
            channels,
            preamp: 10f32.powf(profile.preamp_db / 20.0),
            states: vec![vec![State::default(); coefficients.len()]; channels],
            coefficients,
        }
    }

    /// Обрабатывает блок interleaved-сэмплов на месте.
    pub fn process(&mut self, samples: &mut [f32]) {
        let last = self.states.len() - 1;
        for frame in samples.chunks_mut(self.channels) {
            for (channel, sample) in frame.iter_mut().enumerate() {
                let states = &mut self.states[channel.min(last)];
                let mut value = *sample * self.preamp;
                for (state, coefficients) in states.iter_mut().zip(&self.coefficients) {
                    value = state.step(value, coefficients);
                }
                *sample = value;
            }
        }
    }

    pub fn reset(&mut self) {
        for states in &mut self.states {
            states.fill(State::default());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::PeqFilter;
    use std::f32::consts::PI;

    fn level_db(shape: FilterShape, probe: f32) -> f32 {
        let rate = 48_000.0;
        let profile = HeadphoneProfile {
            name: "тест".into(),
            source: String::new(),
            preamp_db: 0.0,
            filters: vec![PeqFilter { shape, frequency: 1_000.0, gain_db: 6.0, q: 0.7 }],
        };
        let mut eq = ParametricEq::new(&profile, 48_000, 1);
        let mut samples: Vec<f32> =
            (0..48_000).map(|i| (2.0 * PI * probe * i as f32 / rate).sin() * 0.25).collect();
        eq.process(&mut samples);
        let tail = &samples[24_000..];
        let peak = tail.iter().fold(0.0f32, |max, value| max.max(value.abs()));
        20.0 * (peak / 0.25).log10()
    }

    /// Полка поднимает свою сторону на всё усиление, а другую не трогает.
    #[test]
    fn shelves_lift_their_side() {
        assert!((level_db(FilterShape::LowShelf, 50.0) - 6.0).abs() < 0.3);
        assert!(level_db(FilterShape::LowShelf, 15_000.0).abs() < 0.3);
        assert!((level_db(FilterShape::HighShelf, 15_000.0) - 6.0).abs() < 0.3);
        assert!(level_db(FilterShape::HighShelf, 50.0).abs() < 0.3);
    }

    #[test]
    fn peak_lifts_its_frequency_only() {
        assert!((level_db(FilterShape::Peak, 1_000.0) - 6.0).abs() < 0.3);
        assert!(level_db(FilterShape::Peak, 50.0).abs() < 0.5);
    }
}
