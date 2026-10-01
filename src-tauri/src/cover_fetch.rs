//! Обложки из интернета для альбомов без картинки — с записью в файлы.
//!
//! Где ищем: сначала iTunes Search (крупные картинки, хорошо знает и
//! русскую сцену), потом MusicBrainz + Cover Art Archive. Найденное
//! сверяется с тегами по названию и исполнителю: лучше не найти ничего, чем
//! вшить чужую обложку. Картинка вшивается в теги каждого трека альбома;
//! формат без картинок в тегах получает `cover.jpg` рядом.

use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use lofty::config::WriteOptions;
use lofty::file::TaggedFileExt;
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::tag::{Tag, TagExt};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{RingloftError, Result};

const TIMEOUT: Duration = Duration::from_secs(20);
const IMAGE_LIMIT: u64 = 12 * 1024 * 1024;
/// Меньше этого — уже не обложка, а значок.
const MIN_SIDE: u32 = 250;

/// Что вышло с одним альбомом.
#[derive(Debug, Clone, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "CoverFetchResult.ts")]
pub struct CoverFetchResult {
    pub found: bool,
    /// «iTunes» или «Cover Art Archive».
    pub source: Option<String>,
    pub written: u32,
    pub failed: u32,
}

pub struct Found {
    pub bytes: Vec<u8>,
    pub mime: MimeType,
    pub source: &'static str,
}

/// Ищет обложку альбома. `None` — нигде не нашлось подходящей.
pub fn find(artist: &str, album: &str) -> Result<Option<Found>> {
    if album.trim().is_empty() {
        return Ok(None);
    }
    if let Some(found) = from_itunes(artist, album)? {
        return Ok(Some(found));
    }
    from_cover_art_archive(artist, album)
}

/// Вшивает картинку во все файлы. Файлам без тегов с картинками — `cover.jpg`
/// рядом (если в папке ещё нет своей).
pub fn embed(paths: &[String], found: &Found) -> CoverFetchResult {
    let mut result = CoverFetchResult {
        found: true,
        source: Some(found.source.to_owned()),
        ..CoverFetchResult::default()
    };
    for path in paths {
        let path = Path::new(path);
        match embed_one(path, found) {
            Ok(()) => result.written += 1,
            Err(err) => {
                tracing::debug!(%err, path = %path.display(), "обложка в теги не легла");
                if write_sidecar(path, found) {
                    result.written += 1;
                } else {
                    result.failed += 1;
                }
            }
        }
    }
    result
}

fn embed_one(path: &Path, found: &Found) -> Result<()> {
    let tags_error = |message: String| RingloftError::Tags {
        path: path.display().to_string(),
        message,
    };
    let mut file = lofty::read_from_path(path).map_err(|err| tags_error(err.to_string()))?;
    if file.primary_tag_mut().is_none() {
        let kind = file.file_type().primary_tag_type();
        file.insert_tag(Tag::new(kind));
    }
    let tag = file
        .primary_tag_mut()
        .ok_or_else(|| tags_error("в этот формат нельзя записать теги".to_owned()))?;
    tag.remove_picture_type(PictureType::CoverFront);
    tag.push_picture(
        Picture::unchecked(found.bytes.clone())
            .pic_type(PictureType::CoverFront)
            .mime_type(found.mime.clone())
            .build(),
    );
    tag.save_to_path(path, WriteOptions::default())
        .map_err(|err| tags_error(err.to_string()))
}

fn write_sidecar(track: &Path, found: &Found) -> bool {
    let Some(folder) = track.parent() else { return false };
    let extension = if found.mime == MimeType::Png { "png" } else { "jpg" };
    let target = folder.join(format!("cover.{extension}"));
    if target.exists() {
        // Своя картинка в папке уже лежит — значит, файл просто не умеет теги.
        return false;
    }
    std::fs::write(&target, &found.bytes).is_ok()
}

static ITUNES_LAST: Mutex<Option<Instant>> = Mutex::new(None);
static MUSICBRAINZ_LAST: Mutex<Option<Instant>> = Mutex::new(None);

/// Пауза между запросами к одному сервису: у iTunes предел около двадцати
/// запросов в минуту, у MusicBrainz — один в секунду.
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
struct ItunesResponse {
    #[serde(default)]
    results: Vec<ItunesAlbum>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItunesAlbum {
    collection_name: Option<String>,
    artist_name: Option<String>,
    artwork_url100: Option<String>,
}

fn from_itunes(artist: &str, album: &str) -> Result<Option<Found>> {
    pace(&ITUNES_LAST, Duration::from_millis(3100));
    let term = encode(&format!("{artist} {album}"));
    let url = format!("https://itunes.apple.com/search?term={term}&entity=album&limit=10");
    let Some(body) = crate::net::get_text("iTunes", &url, TIMEOUT)? else {
        return Ok(None);
    };
    let response: ItunesResponse = serde_json::from_str(&body)
        .map_err(|err| RingloftError::Network(format!("iTunes: непонятный ответ ({err})")))?;
    let Some(hit) = response.results.into_iter().find(|hit| {
        matches(hit.collection_name.as_deref(), album) && artist_matches(hit.artist_name.as_deref(), artist)
    }) else {
        return Ok(None);
    };
    let Some(small) = hit.artwork_url100 else { return Ok(None) };
    // Адрес миниатюры 100×100 отдаёт и большие размеры, если их попросить.
    let large = small.replace("100x100bb", "1000x1000bb");
    download(&large, "iTunes")
}

#[derive(Deserialize)]
struct MbSearch {
    #[serde(default, rename = "release-groups")]
    groups: Vec<MbGroup>,
}

#[derive(Deserialize)]
struct MbGroup {
    id: String,
    title: Option<String>,
    #[serde(default)]
    score: u32,
    #[serde(default, rename = "artist-credit")]
    credit: Vec<MbCredit>,
}

#[derive(Deserialize)]
struct MbCredit {
    name: String,
}

fn from_cover_art_archive(artist: &str, album: &str) -> Result<Option<Found>> {
    pace(&MUSICBRAINZ_LAST, Duration::from_millis(1100));
    let query = encode(&format!(
        "releasegroup:\"{}\" AND artist:\"{}\"",
        lucene(album),
        lucene(artist)
    ));
    let url = format!("https://musicbrainz.org/ws/2/release-group/?query={query}&fmt=json&limit=5");
    let Some(body) = crate::net::get_text("MusicBrainz", &url, TIMEOUT)? else {
        return Ok(None);
    };
    let search: MbSearch = serde_json::from_str(&body)
        .map_err(|err| RingloftError::Network(format!("MusicBrainz: непонятный ответ ({err})")))?;
    for group in search.groups.into_iter().filter(|group| group.score >= 85) {
        let credit: Vec<&str> = group.credit.iter().map(|credit| credit.name.as_str()).collect();
        if !matches(group.title.as_deref(), album) || !artist_matches(Some(&credit.join(" ")), artist) {
            continue;
        }
        let url = format!("https://coverartarchive.org/release-group/{}/front-1200", group.id);
        if let Some(found) = download(&url, "Cover Art Archive")? {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

fn download(url: &str, source: &'static str) -> Result<Option<Found>> {
    let Some(bytes) = crate::net::get_bytes(source, url, TIMEOUT, IMAGE_LIMIT)? else {
        return Ok(None);
    };
    let format = image::guess_format(&bytes).ok();
    let mime = match format {
        Some(image::ImageFormat::Jpeg) => MimeType::Jpeg,
        Some(image::ImageFormat::Png) => MimeType::Png,
        _ => return Ok(None),
    };
    // Проверяем, что это действительно картинка и не крошечная.
    let Ok(decoded) = image::load_from_memory(&bytes) else {
        return Ok(None);
    };
    if decoded.width().min(decoded.height()) < MIN_SIDE {
        return Ok(None);
    }
    Ok(Some(Found { bytes, mime, source }))
}

/// Сравнение без регистра, пробелов и знаков: «Ballads 1» и «BALLADS 1»
/// совпадают; издания «(Deluxe)» тоже засчитываются — начало то же.
fn matches(found: Option<&str>, wanted: &str) -> bool {
    let (Some(found), wanted) = (found.map(normalize), normalize(wanted)) else {
        return false;
    };
    !wanted.is_empty() && (found == wanted || found.starts_with(&wanted) || wanted.starts_with(&found) && found.len() >= 3)
}

/// Исполнитель: достаточно, чтобы одно содержало другое («ЛСП» и
/// «ЛСП & Feduk»). Неизвестного исполнителя (`artist` пуст) не проверяем.
fn artist_matches(found: Option<&str>, wanted: &str) -> bool {
    let wanted = normalize(wanted);
    if wanted.is_empty() {
        return true;
    }
    found
        .map(normalize)
        .is_some_and(|found| found.contains(&wanted) || wanted.contains(&found) && found.len() >= 2)
}

fn normalize(text: &str) -> String {
    text.chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Кавычки и обратная черта внутри запроса MusicBrainz экранируются.
fn lucene(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Процентная кодировка для адреса.
fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(char::from(byte)),
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_titles_loosely_but_not_blindly() {
        assert!(matches(Some("BALLADS 1"), "Ballads 1"));
        assert!(matches(Some("Magic City (Deluxe)"), "Magic City"));
        assert!(!matches(Some("Magic"), "Ballads 1"));
        assert!(artist_matches(Some("ЛСП & Feduk"), "ЛСП"));
        assert!(!artist_matches(Some("Joji"), "ЛСП"));
        assert!(artist_matches(Some("кто угодно"), ""));
    }

    #[test]
    fn encodes_queries() {
        assert_eq!(encode("ЛСП Magic City"), "%D0%9B%D0%A1%D0%9F+Magic+City");
        assert_eq!(lucene("a \"b\""), "a \\\"b\\\"");
    }

    /// Картинка ложится в теги и читается обратно тем же путём, что у
    /// галереи и нижней панели.
    #[test]
    fn embeds_cover_into_file() {
        let path = crate::audio::test_wav::temp_path("cover-embed.wav");
        crate::audio::test_wav::sine(&path, 44_100, 2, 4_410, 0.2);
        let mut bytes = Vec::new();
        image::RgbImage::from_pixel(300, 300, image::Rgb([200, 40, 40]))
            .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Jpeg)
            .expect("картинка");
        let found = Found { bytes: bytes.clone(), mime: MimeType::Jpeg, source: "тест" };

        let result = embed(&[path.to_string_lossy().into_owned()], &found);
        assert_eq!((result.written, result.failed), (1, 0));
        let file = lofty::read_from_path(&path).expect("чтение");
        let picture = file
            .tags()
            .iter()
            .flat_map(|tag| tag.pictures())
            .find(|picture| picture.pic_type() == PictureType::CoverFront)
            .expect("обложка в тегах");
        assert_eq!(picture.data(), bytes.as_slice());
        let _ = std::fs::remove_file(&path);
    }

    /// Живой поиск: `cargo test -- --ignored live_cover`.
    #[test]
    #[ignore = "ходит в сеть"]
    fn live_cover() {
        let found = find("Joji", "Ballads 1").expect("поиск").expect("обложка");
        assert!(found.bytes.len() > 20_000);
    }
}
