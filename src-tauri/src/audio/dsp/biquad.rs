//! Биквад-фильтры по RBJ cookbook — общие для графического эквалайзера и
//! параметрического профиля наушников (AutoEQ).

use std::f32::consts::PI;

/// Вид фильтра. Названия — как в файлах AutoEQ: PK, LSC, HSC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Peak,
    LowShelf,
    HighShelf,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Coefficients {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Coefficients {
    /// «Провод»: сигнал проходит как есть.
    pub fn passthrough() -> Self {
        Self {
            b0: 1.0,
            ..Self::default()
        }
    }

    /// Нулевое усиление и частота выше Найквиста вырождаются в «провод».
    pub fn new(shape: Shape, frequency: f32, gain_db: f32, q: f32, sample_rate: f32) -> Self {
        if gain_db.abs() < 1e-4 || frequency >= sample_rate / 2.0 || q <= 0.0 {
            return Self::passthrough();
        }

        let amplitude = 10f32.powf(gain_db / 40.0);
        let omega = 2.0 * PI * frequency / sample_rate;
        let alpha = omega.sin() / (2.0 * q);
        let cos_omega = omega.cos();

        let (b0, b1, b2, a0, a1, a2) = match shape {
            Shape::Peak => (
                1.0 + alpha * amplitude,
                -2.0 * cos_omega,
                1.0 - alpha * amplitude,
                1.0 + alpha / amplitude,
                -2.0 * cos_omega,
                1.0 - alpha / amplitude,
            ),
            Shape::LowShelf => {
                let root = 2.0 * amplitude.sqrt() * alpha;
                (
                    amplitude * ((amplitude + 1.0) - (amplitude - 1.0) * cos_omega + root),
                    2.0 * amplitude * ((amplitude - 1.0) - (amplitude + 1.0) * cos_omega),
                    amplitude * ((amplitude + 1.0) - (amplitude - 1.0) * cos_omega - root),
                    (amplitude + 1.0) + (amplitude - 1.0) * cos_omega + root,
                    -2.0 * ((amplitude - 1.0) + (amplitude + 1.0) * cos_omega),
                    (amplitude + 1.0) + (amplitude - 1.0) * cos_omega - root,
                )
            }
            Shape::HighShelf => {
                let root = 2.0 * amplitude.sqrt() * alpha;
                (
                    amplitude * ((amplitude + 1.0) + (amplitude - 1.0) * cos_omega + root),
                    -2.0 * amplitude * ((amplitude - 1.0) + (amplitude + 1.0) * cos_omega),
                    amplitude * ((amplitude + 1.0) + (amplitude - 1.0) * cos_omega - root),
                    (amplitude + 1.0) - (amplitude - 1.0) * cos_omega + root,
                    2.0 * ((amplitude - 1.0) - (amplitude + 1.0) * cos_omega),
                    (amplitude + 1.0) - (amplitude - 1.0) * cos_omega - root,
                )
            }
        };

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }
}

/// Состояние фильтра одного канала (прямая форма II транспонированная).
#[derive(Debug, Clone, Copy, Default)]
pub struct State {
    z1: f32,
    z2: f32,
}

impl State {
    #[inline]
    pub fn step(&mut self, input: f32, coefficients: &Coefficients) -> f32 {
        let output = coefficients.b0 * input + self.z1;
        self.z1 = coefficients.b1 * input - coefficients.a1 * output + self.z2;
        self.z2 = coefficients.b2 * input - coefficients.a2 * output;
        output
    }
}
