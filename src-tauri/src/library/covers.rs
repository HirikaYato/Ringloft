//! Кэш обложек альбомов на диске.
//!
//! Обложка вынимается из первого трека альбома, ужимается до миниатюры и
//! кладётся в `app_cache_dir/covers`. Файлы отдаются вебвью по asset-протоколу:
//! для галереи из сотен картинок это дешевле, чем гонять байты через IPC.

use std::path::{Path, PathBuf};

use image::imageops::FilterType;
use lofty::config::ParseOptions;
use lofty::file::TaggedFileExt;
use lofty::picture::PictureType;
use lofty::probe::Probe;

use super::db::Db;
use crate::error::{RingloftError, Result};

/// Сторона миниатюры. 320 хватает для крупной плитки на HiDPI.
const THUMB_SIZE: u32 = 320;
/// Сколько треков альбома пробуем, если у первого обложки не оказалось.
const TRACKS_TO_TRY: usize = 4;
/// Имена файлов с обложкой рядом с треками — то, что кладут рипперы и torrent-раздачи.
const SIDECAR_NAMES: &[&str] = &[
    "cover", "folder", "front", "album", "albumart", "обложка",
];
const SIDECAR_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp"];

pub struct CoverCache {
    dir: PathBuf,
}

impl CoverCache {
    pub fn new(dir: PathBuf) -> Self {
        if let Err(err) = std::fs::create_dir_all(&dir) {
            tracing::warn!(%err, path = %dir.display(), "не удалось создать кэш обложек");
        }
        Self { dir }
    }

    /// Путь к миниатюре альбома. Извлекает её при первом обращении.
    ///
    /// `Ok(None)` — обложки у альбома нет; рядом остаётся файл-заглушка,
    /// чтобы не перечитывать теги при каждом показе галереи.
    pub fn ensure(&self, db: &Db, album_key: &str) -> Result<Option<PathBuf>> {
        let thumb = self.thumb_path(album_key);
        if thumb.exists() {
            return Ok(Some(thumb));
        }
        if self.miss_path(album_key).exists() {
            return Ok(None);
        }

        let paths = album_track_paths(db, album_key)?;
        for path in paths {
            let Some(picture) = cover_bytes(Path::new(&path)) else {
                continue;
            };
            match write_thumbnail(&picture, &thumb) {
                Ok(()) => {
                    mark_has_cover(db, album_key, true)?;
                    return Ok(Some(thumb));
                }
                Err(err) => {
                    tracing::debug!(%err, path, "обложка не декодировалась");
                }
            }
        }

        let _ = std::fs::write(self.miss_path(album_key), b"");
        mark_has_cover(db, album_key, false)?;
        Ok(None)
    }

    /// Миниатюра для одного файла — нужна системному медиавиджету, который
    /// умеет показывать картинку только по пути к файлу.
    pub fn ensure_for_track(&self, track_path: &Path) -> Option<PathBuf> {
        let key = format!("track:{}", track_path.to_string_lossy());
        let thumb = self.dir.join(format!("{}.jpg", hash_name(&key)));
        if thumb.exists() {
            return Some(thumb);
        }
        let miss = self.dir.join(format!("{}.none", hash_name(&key)));
        if miss.exists() {
            return None;
        }

        match cover_bytes(track_path).map(|data| write_thumbnail(&data, &thumb)) {
            Some(Ok(())) => Some(thumb),
            _ => {
                let _ = std::fs::write(miss, b"");
                None
            }
        }
    }

    /// Обложку вписали в файлы — старую миниатюру и метку «обложки нет»
    /// выбрасываем, следующий показ галереи соберёт её заново.
    pub fn forget(&self, album_key: &str) {
        let _ = std::fs::remove_file(self.thumb_path(album_key));
        let _ = std::fs::remove_file(self.miss_path(album_key));
    }

    fn thumb_path(&self, album_key: &str) -> PathBuf {
        self.dir.join(format!("{}.jpg", hash_name(album_key)))
    }

    fn miss_path(&self, album_key: &str) -> PathBuf {
        self.dir.join(format!("{}.none", hash_name(album_key)))
    }
}

fn album_track_paths(db: &Db, album_key: &str) -> Result<Vec<String>> {
    db.with_read(|connection| {
        let mut statement = connection.prepare_cached(
            "SELECT path FROM tracks WHERE album_key = ?1 ORDER BY disc_no, track_no LIMIT ?2",
        )?;
        let rows = statement
            .query_map(rusqlite::params![album_key, TRACKS_TO_TRY as i64], |row| {
                row.get::<_, String>(0)
            })?;
        let mut paths = Vec::new();
        for row in rows {
            paths.push(row?);
        }
        Ok(paths)
    })
}

fn mark_has_cover(db: &Db, album_key: &str, has_cover: bool) -> Result<()> {
    db.with_write(|connection| {
        connection.execute(
            "UPDATE tracks SET has_cover = ?2 WHERE album_key = ?1",
            rusqlite::params![album_key, i64::from(has_cover)],
        )?;
        Ok(())
    })
}

/// Обложка трека: сначала из тега, потом файл рядом (`cover.jpg` и компания).
fn cover_bytes(path: &Path) -> Option<Vec<u8>> {
    read_picture(path).or_else(|| read_sidecar(path.parent()?))
}

/// Рипы часто кладут картинку рядом с треками, а в теги её не пишут.
fn read_sidecar(folder: &Path) -> Option<Vec<u8>> {
    let entries = std::fs::read_dir(folder).ok()?;
    let mut best: Option<PathBuf> = None;

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let Some(extension) = path.extension().and_then(|ext| ext.to_str()) else {
            continue;
        };
        if !SIDECAR_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()) {
            continue;
        }
        let stem = stem.to_lowercase();
        if SIDECAR_NAMES.iter().any(|name| stem == *name) {
            best = Some(path);
            break;
        }
        if best.is_none() && SIDECAR_NAMES.iter().any(|name| stem.starts_with(name)) {
            best = Some(path);
        }
    }

    std::fs::read(best?).ok()
}

fn read_picture(path: &Path) -> Option<Vec<u8>> {
    let file = Probe::open(path)
        .ok()?
        .options(ParseOptions::new().read_properties(false))
        .read()
        .ok()?;
    let tag = file.primary_tag().or_else(|| file.first_tag())?;
    let picture = tag
        .get_picture_type(PictureType::CoverFront)
        .or_else(|| tag.pictures().first())?;
    Some(picture.data().to_vec())
}

fn write_thumbnail(data: &[u8], target: &Path) -> Result<()> {
    let image = image::load_from_memory(data)
        .map_err(|err| RingloftError::Internal(format!("обложка: {err}")))?;
    // Lanczos3 на уменьшении даёт заметно чище, чем box-фильтр, а считается
    // один раз за альбом.
    let thumb = image.resize(THUMB_SIZE, THUMB_SIZE, FilterType::Lanczos3);
    thumb
        .into_rgb8()
        .save_with_format(target, image::ImageFormat::Jpeg)
        .map_err(|err| RingloftError::Internal(format!("обложка не сохранилась: {err}")))
}

/// FNV-1a: имя файла в кэше должно быть стабильным между запусками,
/// а тянуть ради этого криптографический хеш незачем.
fn hash_name(value: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("ringloft-cover-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("временная папка");
        path
    }

    #[test]
    fn sidecar_prefers_known_names() {
        let dir = temp_dir("sidecar");
        std::fs::write(dir.join("scan_booklet.jpg"), b"booklet").expect("файл");
        std::fs::write(dir.join("cover.jpg"), b"cover").expect("файл");
        assert_eq!(read_sidecar(&dir).as_deref(), Some(&b"cover"[..]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sidecar_ignores_foreign_files() {
        let dir = temp_dir("foreign");
        std::fs::write(dir.join("notes.txt"), b"nope").expect("файл");
        std::fs::write(dir.join("track.flac"), b"nope").expect("файл");
        assert!(read_sidecar(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sidecar_accepts_prefixed_names() {
        let dir = temp_dir("prefixed");
        std::fs::write(dir.join("folder_big.png"), b"art").expect("файл");
        assert_eq!(read_sidecar(&dir).as_deref(), Some(&b"art"[..]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hash_is_stable_and_distinct() {
        assert_eq!(hash_name("kaze\u{1f}blue hour"), hash_name("kaze\u{1f}blue hour"));
        assert_ne!(hash_name("a"), hash_name("b"));
        assert_eq!(hash_name("").len(), 16);
    }
}
