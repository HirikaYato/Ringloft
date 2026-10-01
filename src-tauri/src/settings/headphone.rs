//! Профиль наушников (AutoEQ): параметрический эквалайзер, который
//! выравнивает АЧХ конкретной модели. Хранится в настройках целиком — после
//! загрузки сеть не нужна.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "FilterShape.ts")]
pub enum FilterShape {
    Peak,
    LowShelf,
    HighShelf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "PeqFilter.ts")]
pub struct PeqFilter {
    pub shape: FilterShape,
    pub frequency: f32,
    pub gain_db: f32,
    pub q: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "HeadphoneProfile.ts")]
pub struct HeadphoneProfile {
    /// Модель, как она названа в базе AutoEQ или в имени файла.
    pub name: String,
    /// Кто измерял (oratory1990, crinacle…) — у одной модели бывает несколько.
    pub source: String,
    pub preamp_db: f32,
    pub filters: Vec<PeqFilter>,
}

/// Больше фильтров в профилях AutoEQ не бывает; ограничение — защита от
/// мусорного файла, который положили бы в настройки.
const MAX_FILTERS: usize = 32;

impl HeadphoneProfile {
    /// Всё в разумных пределах: значения едут прямо в коэффициенты фильтров.
    pub fn normalize(&mut self) {
        self.preamp_db = clamp(self.preamp_db, -30.0, 10.0, 0.0);
        self.filters.truncate(MAX_FILTERS);
        self.filters.retain_mut(|filter| {
            filter.frequency = clamp(filter.frequency, 10.0, 24_000.0, 1_000.0);
            filter.gain_db = clamp(filter.gain_db, -30.0, 30.0, 0.0);
            filter.q = clamp(filter.q, 0.05, 20.0, 0.7);
            filter.gain_db != 0.0
        });
    }
}

fn clamp(value: f32, low: f32, high: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(low, high)
    } else {
        fallback
    }
}

/// Разбор `ParametricEQ.txt` из AutoEQ:
///
/// ```text
/// Preamp: -6.1 dB
/// Filter 1: ON LSC Fc 105 Hz Gain 6.4 dB Q 0.70
/// Filter 2: ON PK Fc 8800 Hz Gain 5.1 dB Q 1.42
/// ```
///
/// Выключенные (`OFF`) и незнакомые фильтры пропускаются. `None` — в файле
/// нет ни одного фильтра, то есть это не профиль.
pub fn parse_parametric(text: &str) -> Option<(f32, Vec<PeqFilter>)> {
    let mut preamp = 0.0;
    let mut filters = Vec::new();
    for line in text.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("Preamp:") {
            preamp = number_before(rest, "dB").unwrap_or(0.0);
            continue;
        }
        let Some((_, rest)) = line.split_once(':') else {
            continue;
        };
        let words: Vec<&str> = rest.split_whitespace().collect();
        if words.first() != Some(&"ON") {
            continue;
        }
        let shape = match words.get(1) {
            Some(&"PK") | Some(&"PEQ") => FilterShape::Peak,
            Some(&"LSC") | Some(&"LS") => FilterShape::LowShelf,
            Some(&"HSC") | Some(&"HS") => FilterShape::HighShelf,
            _ => continue,
        };
        let value = |key: &str| {
            words
                .iter()
                .position(|word| *word == key)
                .and_then(|index| words.get(index + 1))
                .and_then(|word| word.parse::<f32>().ok())
        };
        let (Some(frequency), Some(gain_db)) = (value("Fc"), value("Gain")) else {
            continue;
        };
        filters.push(PeqFilter {
            shape,
            frequency,
            gain_db,
            // У полок AutoEQ пишет Q 0.70; если Q нет вовсе — та же величина.
            q: value("Q").unwrap_or(0.707),
        });
    }
    (!filters.is_empty()).then_some((preamp, filters))
}

fn number_before(text: &str, unit: &str) -> Option<f32> {
    text.split(unit).next()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HD650: &str = "Preamp: -6.1 dB\n\
        Filter 1: ON LSC Fc 105 Hz Gain 6.4 dB Q 0.70\n\
        Filter 2: ON PK Fc 8800 Hz Gain 5.1 dB Q 1.42\n\
        Filter 3: OFF PK Fc 118 Hz Gain -3.1 dB Q 0.50\n\
        Filter 4: ON HSC Fc 10000 Hz Gain -2.1 dB Q 0.70\n";

    #[test]
    fn parses_autoeq_parametric_files() {
        let (preamp, filters) = parse_parametric(HD650).expect("профиль");
        assert_eq!(preamp, -6.1);
        assert_eq!(filters.len(), 3, "выключенный фильтр пропущен");
        assert_eq!(filters[0].shape, FilterShape::LowShelf);
        assert_eq!(filters[1].frequency, 8800.0);
        assert_eq!(filters[2].shape, FilterShape::HighShelf);
        assert_eq!(filters[2].gain_db, -2.1);
    }

    #[test]
    fn rejects_files_without_filters() {
        assert!(parse_parametric("Preamp: -3 dB\nпросто текст").is_none());
    }

    #[test]
    fn normalize_keeps_values_sane() {
        let mut profile = HeadphoneProfile {
            name: "x".into(),
            source: String::new(),
            preamp_db: f32::NAN,
            filters: vec![
                PeqFilter { shape: FilterShape::Peak, frequency: 1e9, gain_db: 99.0, q: 0.0 },
                PeqFilter { shape: FilterShape::Peak, frequency: 100.0, gain_db: 0.0, q: 1.0 },
            ],
        };
        profile.normalize();
        assert_eq!(profile.preamp_db, 0.0);
        assert_eq!(profile.filters.len(), 1, "фильтр без усиления не нужен");
        assert_eq!(profile.filters[0].frequency, 24_000.0);
        assert_eq!(profile.filters[0].gain_db, 30.0);
        assert_eq!(profile.filters[0].q, 0.05);
    }
}
