//! Разбор CUE-листов.
//!
//! CUE описывает один образ диска как набор треков с отступами. Время пишется
//! в формате `мм:сс:кк`, где кк — кадры CD по 1/75 секунды. Разбор чистый:
//! никакого ввода-вывода, только текст.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct CueTrack {
    /// Файл-образ, к которому относится трек.
    pub file: PathBuf,
    pub number: u32,
    pub title: String,
    pub artist: Option<String>,
    pub start_ms: u64,
    /// Конец трека — начало следующего в том же файле. У последнего неизвестен,
    /// его подставит импорт по длительности файла.
    pub end_ms: Option<u64>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct CueSheet {
    pub album: Option<String>,
    pub performer: Option<String>,
    pub tracks: Vec<CueTrack>,
}

pub fn parse(content: &str, base_dir: &Path) -> CueSheet {
    let mut sheet = CueSheet::default();
    let mut current_file: Option<PathBuf> = None;
    let mut pending: Option<CueTrack> = None;

    for raw in content.lines() {
        let line = raw.trim_start_matches('\u{feff}').trim();
        let Some((keyword, rest)) = split_keyword(line) else {
            continue;
        };

        match keyword.to_ascii_uppercase().as_str() {
            "FILE" => {
                push(&mut pending, &mut sheet);
                current_file = first_quoted(rest).map(|name| resolve(&name, base_dir));
            }
            "TRACK" => {
                push(&mut pending, &mut sheet);
                let number = rest
                    .split_whitespace()
                    .next()
                    .and_then(|value| value.trim_start_matches('0').parse().ok())
                    .unwrap_or(sheet.tracks.len() as u32 + 1);
                if let Some(file) = current_file.clone() {
                    pending = Some(CueTrack {
                        file,
                        number,
                        title: format!("Трек {number}"),
                        artist: None,
                        start_ms: 0,
                        end_ms: None,
                    });
                }
            }
            "TITLE" => {
                let value = first_quoted(rest).unwrap_or_else(|| rest.to_owned());
                match pending.as_mut() {
                    Some(track) => track.title = value,
                    None => sheet.album = Some(value),
                }
            }
            "PERFORMER" => {
                let value = first_quoted(rest).unwrap_or_else(|| rest.to_owned());
                match pending.as_mut() {
                    Some(track) => track.artist = Some(value),
                    None => sheet.performer = Some(value),
                }
            }
            "INDEX" => {
                // INDEX 00 — предзазор, начало трека задаёт INDEX 01.
                let mut parts = rest.split_whitespace();
                let index = parts.next().unwrap_or_default();
                let time = parts.next().unwrap_or_default();
                if index.trim_start_matches('0').is_empty() {
                    continue;
                }
                if let (Some(track), Some(ms)) = (pending.as_mut(), parse_time(time)) {
                    track.start_ms = ms;
                }
            }
            _ => {}
        }
    }
    push(&mut pending, &mut sheet);

    // Конец каждого трека — начало следующего в том же файле.
    for index in 0..sheet.tracks.len().saturating_sub(1) {
        let next_file = sheet.tracks[index + 1].file.clone();
        let next_start = sheet.tracks[index + 1].start_ms;
        if sheet.tracks[index].file == next_file {
            sheet.tracks[index].end_ms = Some(next_start);
        }
    }

    sheet
}

fn push(pending: &mut Option<CueTrack>, sheet: &mut CueSheet) {
    if let Some(track) = pending.take() {
        sheet.tracks.push(track);
    }
}

fn split_keyword(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.splitn(2, char::is_whitespace);
    let keyword = parts.next()?;
    if keyword.is_empty() {
        return None;
    }
    Some((keyword, parts.next().unwrap_or("").trim()))
}

fn first_quoted(rest: &str) -> Option<String> {
    let start = rest.find('"')?;
    let end = rest[start + 1..].find('"')? + start + 1;
    Some(rest[start + 1..end].to_owned())
}

fn resolve(name: &str, base_dir: &Path) -> PathBuf {
    let normalized = name.replace('\\', "/");
    let path = PathBuf::from(&normalized);
    if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    }
}

/// `мм:сс:кк`, где кк — кадры CD (1/75 секунды).
fn parse_time(value: &str) -> Option<u64> {
    let mut parts = value.split(':');
    let minutes: u64 = parts.next()?.parse().ok()?;
    let seconds: u64 = parts.next()?.parse().ok()?;
    let frames: u64 = parts.next().unwrap_or("0").parse().unwrap_or(0);
    Some((minutes * 60 + seconds) * 1000 + frames * 1000 / 75)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHEET: &str = r#"REM GENRE Ambient
PERFORMER "Kaze no Oto"
TITLE "Первый снег"
FILE "image.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Вступление"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Снег"
    PERFORMER "Гость"
    INDEX 00 03:20:15
    INDEX 01 03:22:00
  TRACK 03 AUDIO
    TITLE "Финал"
    INDEX 01 07:45:37
"#;

    #[test]
    fn parses_tracks_with_boundaries() {
        let sheet = parse(SHEET, Path::new("/album"));
        assert_eq!(sheet.album.as_deref(), Some("Первый снег"));
        assert_eq!(sheet.performer.as_deref(), Some("Kaze no Oto"));
        assert_eq!(sheet.tracks.len(), 3);

        let first = &sheet.tracks[0];
        assert_eq!(first.file, PathBuf::from("/album/image.flac"));
        assert_eq!(first.title, "Вступление");
        assert_eq!(first.start_ms, 0);
        assert_eq!(
            first.end_ms,
            Some(3 * 60_000 + 22_000),
            "конец — начало следующего"
        );

        let second = &sheet.tracks[1];
        assert_eq!(
            second.artist.as_deref(),
            Some("Гость"),
            "исполнитель трека важнее альбомного"
        );
        assert_eq!(
            second.start_ms,
            3 * 60_000 + 22_000,
            "INDEX 00 — предзазор, его игнорируем"
        );

        let last = &sheet.tracks[2];
        assert_eq!(last.start_ms, 7 * 60_000 + 45_000 + 37 * 1000 / 75);
        assert_eq!(last.end_ms, None, "конец последнего трека берётся из файла");
    }

    #[test]
    fn frames_are_seventy_fifths_of_a_second() {
        assert_eq!(parse_time("00:00:75"), Some(1000));
        assert_eq!(parse_time("01:30:00"), Some(90_000));
        assert_eq!(parse_time("мусор"), None);
    }

    #[test]
    fn handles_several_files() {
        let sheet = parse(
            "FILE \"cd1.flac\" WAVE\n TRACK 01 AUDIO\n  INDEX 01 00:00:00\n\
             FILE \"cd2.flac\" WAVE\n TRACK 02 AUDIO\n  INDEX 01 00:00:00\n",
            Path::new("/album"),
        );
        assert_eq!(sheet.tracks.len(), 2);
        assert_eq!(sheet.tracks[0].file, PathBuf::from("/album/cd1.flac"));
        assert_eq!(sheet.tracks[1].file, PathBuf::from("/album/cd2.flac"));
        assert_eq!(
            sheet.tracks[0].end_ms, None,
            "границу между разными файлами не выдумываем"
        );
    }
}
