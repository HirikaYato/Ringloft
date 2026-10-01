//! Профили наушников из базы AutoEQ (github.com/jaakkopasanen/AutoEq).
//!
//! Индекс — markdown на 850 КБ со всеми моделями; кэшируем его на диске на
//! две недели и ищем по нему локально. Профиль (`… ParametricEQ.txt`)
//! скачивается, когда модель выбрали, и дальше живёт в настройках.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{RingloftError, Result};
use crate::settings::{HeadphoneProfile, parse_parametric};

const INDEX_URL: &str = "https://raw.githubusercontent.com/jaakkopasanen/AutoEq/master/results/INDEX.md";
const RESULTS_URL: &str = "https://raw.githubusercontent.com/jaakkopasanen/AutoEq/master/results/";
const TIMEOUT: Duration = Duration::from_secs(20);
const INDEX_MAX_AGE: Duration = Duration::from_secs(14 * 24 * 3600);
const MAX_HITS: usize = 60;

/// Модель в базе. У популярной их несколько — от разных измерителей.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "HeadphoneHit.ts")]
pub struct HeadphoneHit {
    pub name: String,
    /// «oratory1990», «crinacle on GRAS 43AG-7».
    pub source: String,
    /// Путь внутри results/, уже с процентами — как в индексе.
    pub path: String,
}

pub fn search(cache_dir: &Path, query: &str) -> Result<Vec<HeadphoneHit>> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return Ok(Vec::new());
    }
    let index = load_index(cache_dir)?;
    Ok(parse_index(&index)
        .into_iter()
        .filter(|hit| {
            let name = hit.name.to_lowercase();
            words.iter().all(|word| name.contains(word.as_str()))
        })
        .take(MAX_HITS)
        .collect())
}

pub fn download(hit: &HeadphoneHit) -> Result<HeadphoneProfile> {
    let path = hit.path.trim_start_matches("./");
    let file = path.rsplit('/').next().unwrap_or(path);
    let url = format!("{RESULTS_URL}{path}/{file}%20ParametricEQ.txt");
    let text = crate::net::get_text("AutoEQ", &url, TIMEOUT)?
        .ok_or_else(|| RingloftError::Network(format!("у «{}» нет параметрического профиля", hit.name)))?;
    let (preamp_db, filters) = parse_parametric(&text)
        .ok_or_else(|| RingloftError::Network("AutoEQ прислал файл без фильтров".into()))?;
    let mut profile = HeadphoneProfile {
        name: hit.name.clone(),
        source: hit.source.clone(),
        preamp_db,
        filters,
    };
    profile.normalize();
    Ok(profile)
}

/// Свой файл в формате AutoEQ — например, сделанный на autoeq.app.
pub fn import(path: &Path) -> Result<HeadphoneProfile> {
    let bytes = std::fs::read(path).map_err(|err| RingloftError::io(path, err))?;
    let text = crate::text::decode(bytes);
    let (preamp_db, filters) = parse_parametric(&text).ok_or_else(|| {
        RingloftError::UnsupportedFormat("в файле нет фильтров в формате AutoEQ (ParametricEQ.txt)".into())
    })?;
    let stem = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("Свой профиль");
    let mut profile = HeadphoneProfile {
        name: stem.trim_end_matches(" ParametricEQ").to_owned(),
        source: "файл".to_owned(),
        preamp_db,
        filters,
    };
    profile.normalize();
    Ok(profile)
}

fn load_index(cache_dir: &Path) -> Result<String> {
    let cached: PathBuf = cache_dir.join("autoeq-index.md");
    let fresh = std::fs::metadata(&cached)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age < INDEX_MAX_AGE);
    if fresh && let Ok(text) = std::fs::read_to_string(&cached) {
        return Ok(text);
    }

    match crate::net::get_text("AutoEQ", INDEX_URL, TIMEOUT) {
        Ok(Some(text)) => {
            if let Err(err) = crate::atomic_file::write_atomic(&cached, &text) {
                tracing::debug!(%err, "индекс AutoEQ не сохранился в кэш");
            }
            Ok(text)
        }
        // Нет сети — старый индекс лучше никакого.
        other => match std::fs::read_to_string(&cached) {
            Ok(text) => Ok(text),
            Err(_) => match other {
                Err(err) => Err(err),
                _ => Err(RingloftError::Network("AutoEQ: индекс не нашёлся".into())),
            },
        },
    }
}

/// Строки индекса: `- [Название](./источник/вид/Название) by Источник on Стенд`.
fn parse_index(text: &str) -> Vec<HeadphoneHit> {
    text.lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("- [")?;
            let (name, rest) = rest.split_once("](")?;
            // В пути бывают свои скобки — «(ANC%20Off)», — поэтому ищем
            // парную, а не первую.
            let (path, rest) = split_at_closing(rest)?;
            let source = rest.trim().strip_prefix("by ").unwrap_or(rest.trim());
            Some(HeadphoneHit {
                name: name.to_owned(),
                source: source.to_owned(),
                path: path.to_owned(),
            })
        })
        .collect()
}

/// Делит строку по скобке, которая закрывает уже открытую (`(` перед ней
/// съедена разбором `](`).
fn split_at_closing(text: &str) -> Option<(&str, &str)> {
    let mut depth = 0usize;
    for (index, ch) in text.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' if depth == 0 => return Some((&text[..index], &text[index + 1..])),
            ')' => depth -= 1,
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Живая база AutoEQ: `cargo test -- --ignored live_autoeq`.
    #[test]
    #[ignore = "ходит в сеть"]
    fn live_autoeq() {
        let cache = std::env::temp_dir().join("ringloft-autoeq-test");
        std::fs::create_dir_all(&cache).expect("папка кэша");
        let hits = search(&cache, "hd 650").expect("поиск");
        let hit = hits
            .iter()
            .find(|hit| hit.source == "oratory1990")
            .expect("HD 650 от oratory1990");
        let profile = download(hit).expect("профиль");
        assert_eq!(profile.name, "Sennheiser HD 650");
        assert!(profile.filters.len() >= 5, "{profile:?}");
        assert!(profile.preamp_db < 0.0);
    }

    #[test]
    fn parses_index_lines() {
        let text = "# Index\n\n\
            - [Sennheiser HD 650](./oratory1990/over-ear/Sennheiser%20HD%20650) by oratory1990\n\
            - [1MORE Aero (ANC Off)](./HypetheSonics/GRAS%20RA0045%20in-ear/1MORE%20Aero%20(ANC%20Off)) by HypetheSonics on GRAS RA0045\n";
        let hits = parse_index(text);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].name, "Sennheiser HD 650");
        assert_eq!(hits[0].source, "oratory1990");
        assert_eq!(hits[0].path, "./oratory1990/over-ear/Sennheiser%20HD%20650");
        // Скобки в названии модели не ломают разбор пути.
        assert_eq!(hits[1].name, "1MORE Aero (ANC Off)");
        assert_eq!(hits[1].path, "./HypetheSonics/GRAS%20RA0045%20in-ear/1MORE%20Aero%20(ANC%20Off)");
        assert_eq!(hits[1].source, "HypetheSonics on GRAS RA0045");
    }
}
