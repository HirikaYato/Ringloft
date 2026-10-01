//! Поиск текста песни на lrclib.net.
//!
//! Ключей и регистрации у них нет — только просят присылать `User-Agent` с
//! именем программы, это и делаем.
//!
//! Две тонкости, которые видно только на живом API:
//!
//! - в базе лежит **несколько версий одного трека** с разной длительностью, и
//!   `/api/get` возвращает ту, что ближе к присланной. Поэтому длительность из
//!   тегов обязательна: иначе метки времени приедут от другого издания и текст
//!   будет уезжать;
//! - точного совпадения часто нет (у нас в теге «feat.», у них нет), и тогда
//!   спасает `/api/search` — но выбирать из его выдачи приходится самим.

use std::time::Duration;

use serde::Deserialize;

use crate::error::{RingloftError, Result};

const GET: &str = "https://lrclib.net/api/get";
const SEARCH: &str = "https://lrclib.net/api/search";
const TIMEOUT: Duration = Duration::from_secs(12);
/// Насколько может разойтись длительность, чтобы метки времени ещё годились.
const SYNC_TOLERANCE_S: u64 = 8;

/// Что знаем о треке из тегов.
#[derive(Debug, Clone)]
pub struct Query {
    pub artist: String,
    pub title: String,
    pub album: Option<String>,
    pub duration_s: Option<u64>,
}

/// Чем закончился поиск. Инструментал — это не «не нашлось»: текста у трека
/// нет и искать его дальше незачем.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Found(Fetched),
    Instrumental,
    Missing,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fetched {
    /// Содержимое для файла `.lrc` — с метками времени или без.
    pub text: String,
    pub synced: bool,
    /// Как трек назван в базе: по нему видно, то ли нашлось.
    pub artist: String,
    pub title: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Record {
    #[serde(rename = "trackName", default)]
    track: String,
    #[serde(rename = "artistName", default)]
    artist: String,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    instrumental: bool,
    #[serde(rename = "syncedLyrics", default)]
    synced: Option<String>,
    #[serde(rename = "plainLyrics", default)]
    plain: Option<String>,
}

pub fn fetch(query: &Query) -> Result<Outcome> {
    let mut records = Vec::new();
    if let Some(exact) = get(query)? {
        records.push(exact);
    }
    if records.is_empty() {
        records = search(query)?;
    }
    Ok(select(records, query.duration_s))
}

/// Точное совпадение. `None` — они ответили 404, это обычное дело.
fn get(query: &Query) -> Result<Option<Record>> {
    let mut request = ureq::get(GET)
        .query("artist_name", &query.artist)
        .query("track_name", &query.title);
    if let Some(album) = &query.album {
        request = request.query("album_name", album);
    }
    if let Some(duration) = query.duration_s {
        request = request.query("duration", duration.to_string());
    }

    let body = send(request)?;
    match body {
        Some(body) => serde_json::from_str::<Record>(&body)
            .map(Some)
            .map_err(|err| RingloftError::Network(format!("lrclib: непонятный ответ ({err})"))),
        None => Ok(None),
    }
}

fn search(query: &Query) -> Result<Vec<Record>> {
    let request = ureq::get(SEARCH)
        .query("artist_name", &query.artist)
        .query("track_name", &query.title);

    let Some(body) = send(request)? else {
        return Ok(Vec::new());
    };
    serde_json::from_str::<Vec<Record>>(&body)
        .map_err(|err| RingloftError::Network(format!("lrclib: непонятный ответ ({err})")))
}

/// Отправляет запрос. `None` — трек не найден (404), это не ошибка.
fn send(request: ureq::RequestBuilder<ureq::typestate::WithoutBody>) -> Result<Option<String>> {
    let mut response = request
        .header(
            "User-Agent",
            crate::net::USER_AGENT,
        )
        .config()
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .build()
        .call()
        .map_err(|err| RingloftError::Network(format!("lrclib: {err}")))?;

    if response.status() == 404 {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(RingloftError::Network(format!(
            "lrclib ответил {}",
            response.status()
        )));
    }

    response
        .body_mut()
        .read_to_string()
        .map(Some)
        .map_err(|err| RingloftError::Network(format!("lrclib: {err}")))
}

/// Выбор из выдачи. Синхронный текст важнее, но только если длительность
/// сошлась: иначе метки уезжают, и слова без меток полезнее.
fn select(records: Vec<Record>, duration_s: Option<u64>) -> Outcome {
    let mut best: Option<(u8, u64, Fetched)> = None;
    let mut instrumental = false;

    for record in records {
        if record.instrumental {
            instrumental = true;
            continue;
        }

        let gap = match (duration_s, record.duration) {
            (Some(ours), Some(theirs)) => ours.abs_diff(theirs as u64),
            // Длительности нет — сравнивать нечем, считаем совпадением.
            _ => 0,
        };
        let synced_fits = record.synced.is_some() && gap <= SYNC_TOLERANCE_S;

        let (text, synced) = match (&record.synced, &record.plain) {
            (Some(synced), _) if synced_fits => (synced.clone(), true),
            (_, Some(plain)) if !plain.trim().is_empty() => (plain.clone(), false),
            (Some(synced), _) => (synced.clone(), true),
            _ => continue,
        };
        if text.trim().is_empty() {
            continue;
        }

        // Ранг: сначала синхронный по времени, потом просто слова.
        let rank = if synced && synced_fits { 2 } else { 1 };
        let better = best
            .as_ref()
            .is_none_or(|(best_rank, best_gap, _)| rank > *best_rank || (rank == *best_rank && gap < *best_gap));
        if better {
            best = Some((
                rank,
                gap,
                Fetched {
                    text,
                    synced,
                    artist: record.artist,
                    title: record.track,
                },
            ));
        }
    }

    match best {
        Some((_, _, fetched)) => Outcome::Found(fetched),
        None if instrumental => Outcome::Instrumental,
        None => Outcome::Missing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(duration: f64, synced: Option<&str>, plain: Option<&str>) -> Record {
        Record {
            track: "Панелька".to_owned(),
            artist: "Хаски".to_owned(),
            duration: Some(duration),
            instrumental: false,
            synced: synced.map(str::to_owned),
            plain: plain.map(str::to_owned),
        }
    }

    /// Синхронный текст важнее — под него написана подсветка строк.
    #[test]
    fn prefers_synced_when_length_matches() {
        let outcome = select(
            vec![
                record(182.0, None, Some("слова")),
                record(183.0, Some("[00:01.00]слова"), Some("слова")),
            ],
            Some(182),
        );
        assert_eq!(
            outcome,
            Outcome::Found(Fetched {
                text: "[00:01.00]слова".to_owned(),
                synced: true,
                artist: "Хаски".to_owned(),
                title: "Панелька".to_owned(),
            })
        );
    }

    /// А вот метки от другого издания уводят текст: у них трек на минуту
    /// длиннее. Тогда полезнее слова без меток.
    #[test]
    fn far_off_timings_lose_to_plain_words() {
        let outcome = select(
            vec![record(240.0, Some("[00:01.00]слова"), Some("слова"))],
            Some(182),
        );
        let Outcome::Found(found) = outcome else {
            panic!("должно было найтись");
        };
        assert!(!found.synced);
        assert_eq!(found.text, "слова");
    }

    /// Из нескольких подходящих берём ближайший по длительности.
    #[test]
    fn closest_length_wins() {
        let outcome = select(
            vec![
                record(186.0, Some("[00:01.00]дальше"), None),
                record(182.0, Some("[00:01.00]ближе"), None),
            ],
            Some(182),
        );
        let Outcome::Found(found) = outcome else {
            panic!("должно было найтись");
        };
        assert_eq!(found.text, "[00:01.00]ближе");
    }

    /// Длительности у нас нет — сравнивать нечем, берём что дали.
    #[test]
    fn works_without_our_duration() {
        let outcome = select(vec![record(999.0, Some("[00:01.00]слова"), None)], None);
        let Outcome::Found(found) = outcome else {
            panic!("должно было найтись");
        };
        assert!(found.synced);
    }

    /// Инструментал — это не «не нашлось»: текста у трека нет.
    #[test]
    fn instrumental_is_its_own_answer() {
        let mut only = record(182.0, None, None);
        only.instrumental = true;
        assert_eq!(select(vec![only], Some(182)), Outcome::Instrumental);
    }

    #[test]
    fn nothing_usable_is_missing() {
        assert_eq!(select(Vec::new(), Some(182)), Outcome::Missing);
        // Пустые строки текстом не считаются.
        assert_eq!(
            select(vec![record(182.0, Some("  "), Some(""))], Some(182)),
            Outcome::Missing
        );
    }
}
