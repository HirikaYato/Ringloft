//! Текст песни: файл `.lrc` рядом с треком или тег внутри файла.
//!
//! Формат `.lrc` — строки вида `[01:23.45] текст`. Одна строка может нести
//! несколько меток (припев), а служебные строки `[ar:…]`, `[ti:…]` к тексту
//! не относятся.

mod lrclib;
mod scan;

pub use lrclib::{Outcome, Query};
pub use scan::LyricsScan;

use std::path::{Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

use crate::error::Result;

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "LyricLine.ts")]
pub struct LyricLine {
    /// Момент, с которого строка звучит. `None` — текст без синхронизации.
    pub at_ms: Option<u64>,
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "Lyrics.ts")]
pub struct Lyrics {
    pub lines: Vec<LyricLine>,
    /// Есть метки времени — можно подсвечивать строку по позиции.
    pub synced: bool,
    /// Текст взят из файла `.lrc`, а не из тега.
    pub from_file: bool,
}

impl Lyrics {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// Путь к `.lrc` рядом с треком: то же имя, другое расширение.
pub fn sidecar(track: &Path) -> PathBuf {
    track.with_extension("lrc")
}

/// `.lrc` рядом с файлом важнее тега: его положили руками и намеренно.
pub fn read(track: &Path) -> Lyrics {
    // У радио нет ни файла рядом, ни тегов.
    if crate::audio::is_stream(track) {
        return Lyrics::default();
    }
    let file = sidecar(track);
    if let Ok(bytes) = std::fs::read(&file) {
        let lyrics = parse(&crate::text::decode(bytes), true);
        if !lyrics.is_empty() {
            return lyrics;
        }
    }

    match crate::tags::read_lyrics(track) {
        Ok(Some(text)) => parse(&text, false),
        Ok(None) => Lyrics::default(),
        Err(err) => {
            tracing::debug!(%err, path = %track.display(), "текст песни не прочитался");
            Lyrics::default()
        }
    }
}

/// Чем закончился поиск текста в интернете.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "LyricsFetchStatus.ts")]
pub enum LyricsFetchStatus {
    Found,
    /// Текста нет и не будет: это инструментал.
    Instrumental,
    /// В базе такого трека не нашлось.
    Missing,
    /// Искать нечем: ни в теге, ни в имени файла не видно исполнителя.
    NoTags,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "LyricsFetch.ts")]
pub struct LyricsFetch {
    pub status: LyricsFetchStatus,
    pub lyrics: Option<Lyrics>,
    /// Текст лёг файлом рядом с треком: значит найдётся и в следующий раз, и
    /// другими плеерами тоже.
    pub saved: bool,
    /// Как трек назван в базе. Если нашлось не то, это видно сразу.
    pub matched: Option<String>,
}

impl LyricsFetch {
    fn nothing(status: LyricsFetchStatus) -> Self {
        Self {
            status,
            lyrics: None,
            saved: false,
            matched: None,
        }
    }
}

/// Ищет текст в интернете и, если нашёлся, кладёт его файлом рядом с треком.
/// Сам по себе плеер в сеть за текстом не ходит — только по кнопке.
pub fn fetch(track: &Path) -> Result<LyricsFetch> {
    let Some(query) = query_for(track) else {
        return Ok(LyricsFetch::nothing(LyricsFetchStatus::NoTags));
    };

    let found = match lrclib::fetch(&query)? {
        Outcome::Found(found) => found,
        Outcome::Instrumental => {
            return Ok(LyricsFetch::nothing(LyricsFetchStatus::Instrumental));
        }
        Outcome::Missing => return Ok(LyricsFetch::nothing(LyricsFetchStatus::Missing)),
    };

    // Когда файл рядом создавать не надо:
    //
    // - чужой `.lrc` уже лежит — его могли положить руками;
    // - в теге текст уже есть, а нашёлся такой же несинхронный. Файл, который
    //   повторяет тег, ничего не добавляет; вот синхронный — добавляет, и
    //   тогда он тег и перебьёт.
    let file = sidecar(track);
    let tagged = crate::tags::read_lyrics(track).ok().flatten().is_some();
    let worth_saving = !file.exists() && (found.synced || !tagged);
    let saved = if worth_saving {
        match std::fs::write(&file, &found.text) {
            Ok(()) => true,
            Err(err) => {
                tracing::warn!(path = %file.display(), %err, "текст рядом с треком не сохранился");
                false
            }
        }
    } else {
        false
    };

    Ok(LyricsFetch {
        status: LyricsFetchStatus::Found,
        lyrics: Some(parse(&found.text, saved)),
        saved,
        matched: Some(format!("{} — {}", found.artist, found.title)),
    })
}

/// По чему искать: сначала теги, а чего в них нет — берём из имени файла
/// («Исполнитель - Трек» в сборниках встречается чаще, чем теги).
fn query_for(track: &Path) -> Option<Query> {
    if crate::audio::is_stream(track) {
        return None;
    }

    let tags = crate::tags::read(track).ok();
    let from_name = track
        .file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(crate::text::split_artist_title);

    let filled = |value: Option<String>| value.filter(|text| !text.trim().is_empty());
    let artist = filled(tags.as_ref().and_then(|tags| tags.artist.clone()))
        .or_else(|| from_name.as_ref().map(|(artist, _)| artist.clone()))?;
    let title = filled(tags.as_ref().and_then(|tags| tags.title.clone()))
        .or_else(|| from_name.as_ref().map(|(_, title)| title.clone()))?;

    Some(Query {
        artist,
        title,
        album: filled(tags.as_ref().and_then(|tags| tags.album.clone())),
        duration_s: tags
            .as_ref()
            .map(|tags| tags.duration_ms / 1000)
            .filter(|seconds| *seconds > 0),
    })
}

/// Есть ли у трека текст — без чтения самого текста.
pub fn exists(track: &Path, tagged: bool) -> bool {
    if crate::audio::is_stream(track) {
        return false;
    }
    tagged || sidecar(track).is_file()
}

fn parse(text: &str, from_file: bool) -> Lyrics {
    let mut lines: Vec<LyricLine> = Vec::new();
    let mut offset_ms: i64 = 0;
    let mut synced = false;

    for raw in text.lines() {
        let mut rest = raw.trim();
        let mut stamps = Vec::new();
        while let Some((at_ms, tail)) = take_stamp(rest) {
            stamps.push(at_ms);
            rest = tail;
        }

        if stamps.is_empty() {
            // Служебные строки вроде `[ar:…]` — не текст. Смещение из
            // `[offset:…]` при этом запоминаем.
            if let Some(value) = take_meta(rest) {
                offset_ms = value.unwrap_or(offset_ms);
                continue;
            }
            lines.push(LyricLine {
                at_ms: None,
                text: rest.to_owned(),
            });
            continue;
        }

        synced = true;
        let body = rest.trim().to_owned();
        for at_ms in stamps {
            lines.push(LyricLine {
                at_ms: Some(shift(at_ms, offset_ms)),
                text: body.clone(),
            });
        }
    }

    if synced {
        // Строки без метки в синхронном тексте — это пустые разделители из
        // файла. Смысла в них уже нет (разрядку даёт сама подсветка), а
        // сортировка бросила бы их в начало.
        lines.retain(|line| line.at_ms.is_some());
        // Метки в файле не обязаны идти по порядку: строки припева часто
        // дописывают в конец.
        lines.sort_by_key(|line| line.at_ms.unwrap_or(0));
    }
    // Хвост из пустых строк не несёт смысла, а место занимает. Кроме
    // синхронного текста: там пустая строка с меткой значит «пение кончилось»,
    // и без неё последняя строка горела бы весь проигрыш.
    while !synced && lines.last().is_some_and(|line| line.text.trim().is_empty()) {
        lines.pop();
    }

    Lyrics {
        lines,
        synced,
        from_file,
    }
}

/// По исходной спецификации `.lrc` «+» в `offset` показывает текст раньше,
/// поэтому смещение вычитается.
fn shift(at_ms: u64, offset_ms: i64) -> u64 {
    (at_ms as i64 - offset_ms).max(0) as u64
}

/// Снимает с начала строки одну метку времени `[mm:ss.xx]`.
fn take_stamp(line: &str) -> Option<(u64, &str)> {
    let rest = line.trim_start();
    let body = rest.strip_prefix('[')?;
    let end = body.find(']')?;
    let (inside, tail) = (&body[..end], &body[end + 1..]);

    let (minutes, seconds) = inside.split_once(':')?;
    let minutes: u64 = minutes.trim().parse().ok()?;
    let (whole, frac) = match seconds.split_once(['.', ':']) {
        Some((whole, frac)) => (whole, Some(frac)),
        None => (seconds, None),
    };
    let whole: u64 = whole.trim().parse().ok()?;

    Some((minutes * 60_000 + whole * 1_000 + frac_ms(frac), tail))
}

/// Доли секунды пишут двумя знаками (сотые), реже тремя (миллисекунды).
fn frac_ms(frac: Option<&str>) -> u64 {
    let Some(frac) = frac else {
        return 0;
    };
    let digits: String = frac.chars().filter(char::is_ascii_digit).collect();
    let value: u64 = digits.parse().unwrap_or(0);
    match digits.len() {
        0 => 0,
        1 => value * 100,
        2 => value * 10,
        _ => value / 10u64.pow(digits.len() as u32 - 3),
    }
}

/// `Some(None)` — служебная строка, `Some(Some(ms))` — она же со смещением.
fn take_meta(line: &str) -> Option<Option<i64>> {
    let body = line.trim().strip_prefix('[')?.strip_suffix(']')?;
    let (key, value) = body.split_once(':')?;
    if !key.chars().all(|ch| ch.is_ascii_alphabetic()) {
        return None;
    }
    if key.eq_ignore_ascii_case("offset") {
        return Some(value.trim().parse().ok());
    }
    Some(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Пустая строка с меткой в конце — «пение кончилось»: без неё последняя
    /// строка горела бы до конца трека.
    #[test]
    fn keeps_the_closing_pause_of_synced_text() {
        let lyrics = parse("[00:01.00]Строка\n[00:05.00]\n\n", true);
        assert_eq!(lyrics.lines.len(), 2);
        assert_eq!(lyrics.lines[1].at_ms, Some(5_000));
        assert!(lyrics.lines[1].text.is_empty());

        let plain = parse("Строка\n\n", true);
        assert_eq!(plain.lines.len(), 1);
    }

    #[test]
    fn parses_timestamps_and_skips_service_lines() {
        let text = "[ti:Ночь]\n[ar:Кишлак]\n[00:12.50]первая строка\n[01:05]вторая\n[00:03.5]ранняя\n";
        let lyrics = parse(text, true);

        assert!(lyrics.synced);
        assert!(lyrics.from_file);
        let stamps: Vec<Option<u64>> = lyrics.lines.iter().map(|line| line.at_ms).collect();
        assert_eq!(
            stamps,
            vec![Some(3_500), Some(12_500), Some(65_000)],
            "строки идут по времени, а не по порядку в файле"
        );
        assert_eq!(lyrics.lines[0].text, "ранняя");
    }

    /// Припев пишут одной строкой с несколькими метками.
    #[test]
    fn one_line_can_have_several_stamps() {
        let lyrics = parse("[00:10.00][00:40.00][01:10.00]припев\n", true);
        assert_eq!(lyrics.lines.len(), 3);
        assert!(lyrics.lines.iter().all(|line| line.text == "припев"));
        assert_eq!(
            lyrics.lines.iter().filter_map(|line| line.at_ms).collect::<Vec<_>>(),
            vec![10_000, 40_000, 70_000]
        );
    }

    /// Пустой разделитель внутри синхронного текста выбрасывается: строка
    /// без метки сломала бы и порядок, и подсветку.
    #[test]
    fn drops_blank_lines_in_synced_text() {
        let lyrics = parse("[00:01.00]первая\n\n[00:05.00]вторая\n", true);
        assert!(lyrics.synced);
        assert_eq!(lyrics.lines.len(), 2);
        assert!(lyrics.lines.iter().all(|line| line.at_ms.is_some()));
    }

    /// Текст без меток остаётся текстом: показать его всё равно надо.
    #[test]
    fn plain_text_stays_unsynced() {
        let lyrics = parse("первая\nвторая\n\n", false);
        assert!(!lyrics.synced);
        assert_eq!(lyrics.lines.len(), 2, "пустой хвост отбрасывается");
        assert!(lyrics.lines.iter().all(|line| line.at_ms.is_none()));
    }

    /// `offset` сдвигает весь текст; «+» по спецификации показывает раньше.
    #[test]
    fn applies_offset() {
        let lyrics = parse("[offset:+500]\n[00:10.00]строка\n", true);
        assert_eq!(lyrics.lines[0].at_ms, Some(9_500));

        let lyrics = parse("[offset:-500]\n[00:10.00]строка\n", true);
        assert_eq!(lyrics.lines[0].at_ms, Some(10_500));

        // Уехать в минус нельзя.
        let lyrics = parse("[offset:+5000]\n[00:01.00]строка\n", true);
        assert_eq!(lyrics.lines[0].at_ms, Some(0));
    }

    /// Доли секунды бывают и в сотых, и в миллисекундах.
    #[test]
    fn reads_fractions() {
        assert_eq!(frac_ms(Some("5")), 500);
        assert_eq!(frac_ms(Some("05")), 50);
        assert_eq!(frac_ms(Some("123")), 123);
        assert_eq!(frac_ms(Some("1234")), 123);
        assert_eq!(frac_ms(None), 0);
    }

    /// Файл рядом важнее тега — и отмечается флагом, чтобы это было видно.
    /// По чему искать, когда тегов нет: имя файла «Исполнитель - Трек»
    /// в сборниках встречается чаще, чем заполненные теги.
    #[test]
    fn query_falls_back_to_the_file_name() {
        let dir = std::env::temp_dir().join(format!("ringloft-lyrics-query-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);

        let named = dir.join("Кишлак - Ночь.wav");
        crate::audio::test_wav::sine(&named, 44_100, 2, 44_100, 0.4);
        let query = query_for(&named).expect("по имени файла искать можно");
        assert_eq!((query.artist.as_str(), query.title.as_str()), ("Кишлак", "Ночь"));

        // Тег важнее имени файла.
        let edit = crate::tag_edit::TagEdit {
            artist: Some("Хаски".to_owned()),
            title: Some("Панелька".to_owned()),
            ..Default::default()
        };
        crate::tag_edit::write(&named, &edit).expect("теги");
        let query = query_for(&named).expect("теперь из тегов");
        assert_eq!((query.artist.as_str(), query.title.as_str()), ("Хаски", "Панелька"));

        // Ни тегов, ни тире в имени — искать нечем.
        let blank = dir.join("track01.wav");
        crate::audio::test_wav::sine(&blank, 44_100, 2, 44_100, 0.4);
        assert!(query_for(&blank).is_none());

        // У радио ни файла, ни тегов.
        assert!(query_for(Path::new("https://example.org/stream")).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sidecar_wins_over_tag() {
        let path = crate::audio::test_wav::temp_path("lyrics-sidecar.wav");
        crate::audio::test_wav::constant(&path, 44_100, 2, 1_000, 0.0);
        let lrc = sidecar(&path);
        std::fs::write(&lrc, "[00:01.00]из файла\n").expect("файл не записался");

        assert!(exists(&path, false), "файл рядом — текст есть");
        let lyrics = read(&path);
        assert!(lyrics.from_file);
        assert!(lyrics.synced);
        assert_eq!(lyrics.lines[0].text, "из файла");

        let _ = std::fs::remove_file(&lrc);
        let _ = std::fs::remove_file(&path);
    }
}

