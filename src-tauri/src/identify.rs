//! Теги по звуку: отпечаток (`audio/fingerprint.rs`) → AcoustID → варианты
//! из MusicBrainz. Нужен ключ приложения AcoustID — его, как ключ Last.fm,
//! заводит пользователь (acoustid.org/new-application), чужой ключ — чужая
//! квота.
//!
//! Плеер ничего не пишет сам: варианты показываются, выбирает пользователь,
//! а записывает обычная правка тегов (`tag_edit.rs`).

use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{RingloftError, Result};

const LOOKUP_URL: &str = "https://api.acoustid.org/v2/lookup";
const META: &str = "recordings releasegroups releases tracks compress";
const TIMEOUT: Duration = Duration::from_secs(20);
const MAX_CANDIDATES: usize = 12;

/// Вариант тегов для файла.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "TagCandidate.ts")]
pub struct TagCandidate {
    /// Насколько отпечаток совпал, 0…1.
    pub score: f32,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    /// Исполнитель альбома, если он не тот же, что у трека (сборники).
    pub album_artist: Option<String>,
    pub year: Option<u32>,
    pub track: Option<u32>,
    pub track_total: Option<u32>,
    /// Номер диска — только у многодисковых изданий.
    pub disk: Option<u32>,
    /// «Альбом», «Сингл», «Сборник» — чтобы было видно, что выбираешь.
    pub kind: Option<String>,
    pub release_group_id: Option<String>,
}

/// Варианты для одного файла: отпечаток и запрос к AcoustID.
pub fn identify(path: &Path, api_key: &str) -> Result<Vec<TagCandidate>> {
    let (duration, fingerprint) = crate::audio::fingerprint::fingerprint(path)?;
    pace(&ACOUSTID_LAST, Duration::from_millis(350));
    let duration = duration.to_string();
    let body = crate::net::post_form_text(
        "AcoustID",
        LOOKUP_URL,
        &[
            ("client", api_key),
            ("duration", &duration),
            ("meta", META),
            ("fingerprint", &fingerprint),
        ],
        TIMEOUT,
    )?;
    let response: Response = serde_json::from_str(&body)
        .map_err(|err| RingloftError::Network(format!("AcoustID: непонятный ответ ({err})")))?;
    if response.status != "ok" {
        let message = response.error.map(|error| error.message).unwrap_or_default();
        return Err(RingloftError::Network(if message.contains("invalid API key") {
            "AcoustID не принял ключ — проверь его в настройках".to_owned()
        } else {
            format!("AcoustID: {message}")
        }));
    }
    Ok(candidates(response.results))
}

/// Жанр группы релизов из MusicBrainz — самый «голосуемый».
pub fn genre(release_group_id: &str) -> Result<Option<String>> {
    if !release_group_id.chars().all(|ch| ch.is_ascii_hexdigit() || ch == '-') {
        return Ok(None);
    }
    // MusicBrainz просит не чаще раза в секунду.
    pace(&MUSICBRAINZ_LAST, Duration::from_millis(1100));
    let url = format!("https://musicbrainz.org/ws/2/release-group/{release_group_id}?inc=genres&fmt=json");
    let Some(body) = crate::net::get_text("MusicBrainz", &url, TIMEOUT)? else {
        return Ok(None);
    };
    #[derive(Deserialize)]
    struct Group {
        #[serde(default)]
        genres: Vec<Genre>,
    }
    #[derive(Deserialize)]
    struct Genre {
        name: String,
        #[serde(default)]
        count: u32,
    }
    let group: Group = serde_json::from_str(&body)
        .map_err(|err| RingloftError::Network(format!("MusicBrainz: непонятный ответ ({err})")))?;
    Ok(group
        .genres
        .into_iter()
        .max_by_key(|genre| genre.count)
        .map(|genre| genre.name))
}

static ACOUSTID_LAST: Mutex<Option<Instant>> = Mutex::new(None);
static MUSICBRAINZ_LAST: Mutex<Option<Instant>> = Mutex::new(None);

/// Держит паузу между запросами к одному сервису: их правила частоты строгие.
fn pace(last: &Mutex<Option<Instant>>, gap: Duration) {
    let Ok(mut last) = last.lock() else { return };
    if let Some(previous) = *last {
        let elapsed = previous.elapsed();
        if elapsed < gap {
            std::thread::sleep(gap - elapsed);
        }
    }
    *last = Some(Instant::now());
}

#[derive(Deserialize)]
struct Response {
    status: String,
    #[serde(default)]
    results: Vec<LookupResult>,
    error: Option<ApiError>,
}

#[derive(Deserialize)]
struct ApiError {
    message: String,
}

#[derive(Deserialize)]
struct LookupResult {
    score: f32,
    #[serde(default)]
    recordings: Vec<Recording>,
}

#[derive(Deserialize)]
struct Recording {
    title: Option<String>,
    #[serde(default)]
    artists: Vec<Artist>,
    #[serde(default)]
    releasegroups: Vec<ReleaseGroup>,
}

#[derive(Deserialize)]
struct Artist {
    name: String,
    joinphrase: Option<String>,
}

#[derive(Deserialize)]
struct ReleaseGroup {
    id: String,
    title: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    secondarytypes: Vec<String>,
    #[serde(default)]
    releases: Vec<Release>,
}

#[derive(Deserialize)]
struct Release {
    date: Option<Date>,
    #[serde(default)]
    artists: Vec<Artist>,
    medium_count: Option<u32>,
    #[serde(default)]
    mediums: Vec<Medium>,
}

#[derive(Deserialize)]
struct Date {
    year: Option<u32>,
}

#[derive(Deserialize)]
struct Medium {
    position: Option<u32>,
    track_count: Option<u32>,
    #[serde(default)]
    tracks: Vec<Track>,
}

#[derive(Deserialize)]
struct Track {
    position: Option<u32>,
}

fn credit(artists: &[Artist]) -> String {
    let mut text = String::new();
    for artist in artists {
        text.push_str(&artist.name);
        text.push_str(artist.joinphrase.as_deref().unwrap_or(""));
    }
    text.trim().to_owned()
}

/// Из ответа AcoustID — варианты «запись × альбом». У альбома берём самое
/// раннее издание: год оригинала обычно и нужен, а не переиздания.
fn candidates(results: Vec<LookupResult>) -> Vec<TagCandidate> {
    let mut out: Vec<TagCandidate> = Vec::new();
    for result in results {
        for recording in result.recordings {
            let Some(title) = recording.title.clone() else { continue };
            let artist = credit(&recording.artists);
            if recording.releasegroups.is_empty() {
                out.push(TagCandidate {
                    score: result.score,
                    title: title.clone(),
                    artist: artist.clone(),
                    album: None,
                    album_artist: None,
                    year: None,
                    track: None,
                    track_total: None,
                    disk: None,
                    kind: None,
                    release_group_id: None,
                });
            }
            for group in &recording.releasegroups {
                let earliest = group
                    .releases
                    .iter()
                    .min_by_key(|release| release.date.as_ref().and_then(|date| date.year).unwrap_or(u32::MAX));
                let medium = earliest.and_then(|release| release.mediums.first());
                let album_artist = earliest
                    .map(|release| credit(&release.artists))
                    .filter(|name| !name.is_empty() && *name != artist);
                let mut kind: Vec<String> = group.kind.iter().cloned().collect();
                kind.extend(group.secondarytypes.iter().cloned());
                out.push(TagCandidate {
                    score: result.score,
                    title: title.clone(),
                    artist: artist.clone(),
                    album: group.title.clone(),
                    album_artist,
                    year: earliest.and_then(|release| release.date.as_ref()).and_then(|date| date.year),
                    track: medium.and_then(|medium| medium.tracks.first()).and_then(|track| track.position),
                    track_total: medium.and_then(|medium| medium.track_count),
                    disk: earliest
                        .filter(|release| release.medium_count.unwrap_or(1) > 1)
                        .and(medium)
                        .and_then(|medium| medium.position),
                    kind: (!kind.is_empty()).then(|| translate_kind(&kind)),
                    release_group_id: Some(group.id.clone()),
                });
            }
        }
    }

    // Сначала уверенные совпадения, среди них — настоящие альбомы, потом
    // синглы и сборники; при равенстве — что раньше вышло.
    out.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| kind_rank(a.kind.as_deref()).cmp(&kind_rank(b.kind.as_deref())))
            .then_with(|| a.year.unwrap_or(u32::MAX).cmp(&b.year.unwrap_or(u32::MAX)))
    });
    let mut seen = std::collections::HashSet::new();
    out.retain(|candidate| {
        seen.insert((candidate.title.clone(), candidate.artist.clone(), candidate.album.clone()))
    });
    out.truncate(MAX_CANDIDATES);
    out
}

fn translate_kind(kinds: &[String]) -> String {
    kinds
        .iter()
        .map(|kind| match kind.as_str() {
            "Album" => "альбом",
            "Single" => "сингл",
            "EP" => "мини-альбом",
            "Compilation" => "сборник",
            "Soundtrack" => "саундтрек",
            "Live" => "концерт",
            "Remix" => "ремиксы",
            other => other,
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn kind_rank(kind: Option<&str>) -> u8 {
    match kind {
        Some("альбом") => 0,
        Some("мини-альбом") => 1,
        Some("сингл") => 2,
        Some(_) => 3,
        None => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"status":"ok","results":[{"score":0.97,"recordings":[{"title":"Attention",
        "artists":[{"name":"Joji"}],"releasegroups":[
          {"id":"rg-single","title":"Attention","type":"Single","releases":[{"date":{"year":2018},"mediums":[{"position":1,"track_count":1,"tracks":[{"position":1}]}]}]},
          {"id":"rg-album","title":"Ballads 1","type":"Album","releases":[
            {"date":{"year":2019},"mediums":[{"position":1,"track_count":12,"tracks":[{"position":1}]}]},
            {"date":{"year":2018},"artists":[{"name":"Joji"}],"medium_count":1,"mediums":[{"position":1,"track_count":12,"tracks":[{"position":1}]}]}]},
          {"id":"rg-comp","title":"Best of 2018","type":"Album","secondarytypes":["Compilation"],"releases":[{"date":{"year":2018},"artists":[{"name":"Various Artists"}],"mediums":[{"position":1,"track_count":40,"tracks":[{"position":7}]}]}]}
        ]}]}]}"#;

    #[test]
    fn builds_candidates_preferring_albums() {
        let response: Response = serde_json::from_str(SAMPLE).expect("разбор");
        let list = candidates(response.results);
        assert_eq!(list.len(), 3);
        let first = &list[0];
        assert_eq!(first.album.as_deref(), Some("Ballads 1"));
        assert_eq!(first.year, Some(2018), "самое раннее издание");
        assert_eq!(first.track, Some(1));
        assert_eq!(first.track_total, Some(12));
        assert_eq!(first.album_artist, None, "тот же исполнитель — не дублируем");
        assert_eq!(first.kind.as_deref(), Some("альбом"));
        assert_eq!(list[1].kind.as_deref(), Some("сингл"));
        let compilation = &list[2];
        assert_eq!(compilation.album_artist.as_deref(), Some("Various Artists"));
        assert_eq!(compilation.kind.as_deref(), Some("альбом, сборник"));
    }

    #[test]
    fn reports_api_errors() {
        let response: Response =
            serde_json::from_str(r#"{"status":"error","error":{"code":4,"message":"invalid API key"}}"#)
                .expect("разбор");
        assert_eq!(response.status, "error");
        assert!(response.error.is_some_and(|error| error.message.contains("API key")));
    }
}
