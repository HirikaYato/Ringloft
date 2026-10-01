//! Фоновое сканирование папок библиотеки.
//!
//! Обход диска — один поток (он упирается в ввод-вывод), разбор тегов — rayon
//! пачками, запись — транзакциями по 512 строк. Повторный скан трогает только
//! то, у чего изменились размер или время правки.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossbeam_channel::Sender;
use rayon::prelude::*;
use rusqlite::params;

use super::db::Db;
use super::models::ScannedTrack;
use super::{LibraryEvent, ScanPhase, ScanProgress};
use crate::error::Result;
use crate::playlist::is_audio_path;

/// Размер пачки: столько файлов разбираем параллельно и пишем одной транзакцией.
const BATCH: usize = 512;
/// Чаще дёргать фронт смысла нет — прогресс всё равно рисуется полоской.
const PROGRESS_PERIOD: Duration = Duration::from_millis(200);

#[derive(Debug)]
struct DiskFile {
    path: PathBuf,
    size: u64,
    mtime: i64,
}

pub fn run(db: &Arc<Db>, folders: &[PathBuf], cancel: &Arc<AtomicBool>, events: &Sender<LibraryEvent>) {
    let started = Instant::now();
    match scan(db, folders, cancel, events) {
        Ok(Outcome::Done { added, updated, removed }) => {
            tracing::info!(
                added,
                updated,
                removed,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "сканирование завершено"
            );
            send(events, LibraryEvent::Finished { added, updated, removed });
        }
        Ok(Outcome::Cancelled) => {
            tracing::info!("сканирование отменено");
            send(events, LibraryEvent::Progress(ScanProgress::phase(ScanPhase::Cancelled)));
        }
        Err(err) => {
            tracing::error!(%err, "сканирование не удалось");
            send(events, LibraryEvent::Failed((&err).into()));
        }
    }
}

enum Outcome {
    Done { added: u32, updated: u32, removed: u32 },
    Cancelled,
}

fn scan(
    db: &Arc<Db>,
    folders: &[PathBuf],
    cancel: &Arc<AtomicBool>,
    events: &Sender<LibraryEvent>,
) -> Result<Outcome> {
    // 1. Обходим диск.
    send(events, LibraryEvent::Progress(ScanProgress::phase(ScanPhase::Walking)));
    let files = walk(folders, cancel, events);
    if cancel.load(Ordering::Relaxed) {
        return Ok(Outcome::Cancelled);
    }

    // 2. Что уже лежит в базе.
    let known = load_known(db)?;
    let on_disk: HashSet<&str> = files.iter().filter_map(|file| file.path.to_str()).collect();

    // 3. Разбираем только новое и изменившееся.
    let pending: Vec<&DiskFile> = files
        .iter()
        .filter(|file| match file.path.to_str().and_then(|path| known.get(path)) {
            Some((mtime, size)) => *mtime != file.mtime || *size != file.size as i64,
            None => true,
        })
        .collect();

    let total = pending.len() as u32;
    let mut added = 0u32;
    let mut updated = 0u32;
    let mut processed = 0u32;
    let mut last_report = Instant::now();

    for chunk in pending.chunks(BATCH) {
        if cancel.load(Ordering::Relaxed) {
            return Ok(Outcome::Cancelled);
        }

        let parsed: Vec<ScannedTrack> = chunk
            .par_iter()
            .filter_map(|file| ScannedTrack::read(&file.path, file.size, file.mtime))
            .collect();

        let existing = &known;
        for track in &parsed {
            if existing.contains_key(track.path.as_str()) {
                updated += 1;
            } else {
                added += 1;
            }
        }

        write_batch(db, &parsed)?;
        processed += chunk.len() as u32;

        if last_report.elapsed() >= PROGRESS_PERIOD {
            last_report = Instant::now();
            send(
                events,
                LibraryEvent::Progress(ScanProgress {
                    phase: ScanPhase::Reading,
                    processed,
                    total,
                    current: parsed.last().map(|track| track.path.clone()),
                }),
            );
        }
    }

    // 4. Чистим то, чего на диске больше нет.
    send(events, LibraryEvent::Progress(ScanProgress::phase(ScanPhase::Cleaning)));
    let gone: Vec<String> = known
        .keys()
        .filter(|path| !on_disk.contains(path.as_str()))
        .filter(|path| belongs_to(path, folders))
        .cloned()
        .collect();
    let removed = delete_missing(db, &gone)?;

    Ok(Outcome::Done { added, updated, removed })
}

fn walk(folders: &[PathBuf], cancel: &Arc<AtomicBool>, events: &Sender<LibraryEvent>) -> Vec<DiskFile> {
    let mut files = Vec::new();
    let mut last_report = Instant::now();

    for folder in folders {
        for entry in walkdir::WalkDir::new(folder)
            .follow_links(false)
            .into_iter()
            .filter_map(|entry| entry.ok())
        {
            if cancel.load(Ordering::Relaxed) {
                return files;
            }
            if !entry.file_type().is_file() || !is_audio_path(entry.path()) {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            files.push(DiskFile {
                path: entry.path().to_path_buf(),
                size: metadata.len(),
                mtime: unix_seconds(metadata.modified().ok()),
            });

            if last_report.elapsed() >= PROGRESS_PERIOD {
                last_report = Instant::now();
                send(
                    events,
                    LibraryEvent::Progress(ScanProgress {
                        phase: ScanPhase::Walking,
                        processed: files.len() as u32,
                        total: 0,
                        current: None,
                    }),
                );
            }
        }
    }
    files
}

/// path -> (mtime, size). Держим в памяти: на 50 000 треков это единицы мегабайт,
/// зато сравнение идёт без обращения к базе на каждый файл.
fn load_known(db: &Arc<Db>) -> Result<HashMap<String, (i64, i64)>> {
    db.with_read(|connection| {
        let mut statement = connection.prepare("SELECT path, mtime, size_bytes FROM tracks")?;
        let mut rows = statement.query([])?;
        let mut known = HashMap::new();
        while let Some(row) = rows.next()? {
            known.insert(row.get::<_, String>(0)?, (row.get(1)?, row.get(2)?));
        }
        Ok(known)
    })
}

fn write_batch(db: &Arc<Db>, tracks: &[ScannedTrack]) -> Result<()> {
    if tracks.is_empty() {
        return Ok(());
    }
    let now = unix_seconds(Some(SystemTime::now()));

    db.with_write(|connection| {
        let transaction = connection.transaction()?;
        {
            let mut statement = transaction.prepare_cached(UPSERT)?;
            for track in tracks {
                statement.execute(params![
                    track.path,
                    track.folder,
                    track.title,
                    track.artist,
                    track.album_artist,
                    track.album,
                    track.album_key,
                    track.genre,
                    track.year,
                    track.track_no,
                    track.disc_no,
                    // u64 в SQLite не влезает по типам: переводим в i64 явно.
                    track.duration_ms.map(|value| value as i64),
                    track.sample_rate,
                    track.channels,
                    track.bitrate_kbps,
                    track.size_bytes as i64,
                    track.mtime,
                    now,
                ])?;
            }
        }
        transaction.commit()?;
        Ok(())
    })
}

fn delete_missing(db: &Arc<Db>, paths: &[String]) -> Result<u32> {
    if paths.is_empty() {
        return Ok(0);
    }
    db.with_write(|connection| {
        let transaction = connection.transaction()?;
        let mut removed = 0u32;
        {
            let mut statement = transaction.prepare_cached("DELETE FROM tracks WHERE path = ?1")?;
            for path in paths {
                removed += statement.execute([path])? as u32;
            }
        }
        transaction.commit()?;
        Ok(removed)
    })
}

/// Удалять можно только то, что лежит внутри просканированных папок: иначе
/// скан одной папки вычистил бы из базы всю остальную библиотеку.
fn belongs_to(path: &str, folders: &[PathBuf]) -> bool {
    folders
        .iter()
        .any(|folder| path.starts_with(&*folder.to_string_lossy()))
}

fn unix_seconds(time: Option<SystemTime>) -> i64 {
    time.and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

fn send(events: &Sender<LibraryEvent>, event: LibraryEvent) {
    let _ = events.send(event);
}

const UPSERT: &str = r#"
INSERT INTO tracks (
    path, folder, title, artist, album_artist, album, album_key, genre, year,
    track_no, disc_no, duration_ms, sample_rate, channels, bitrate_kbps,
    size_bytes, mtime, added_at, scanned_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?18)
ON CONFLICT(path) DO UPDATE SET
    folder = excluded.folder, title = excluded.title, artist = excluded.artist,
    album_artist = excluded.album_artist, album = excluded.album,
    album_key = excluded.album_key, genre = excluded.genre, year = excluded.year,
    track_no = excluded.track_no, disc_no = excluded.disc_no,
    duration_ms = excluded.duration_ms, sample_rate = excluded.sample_rate,
    channels = excluded.channels, bitrate_kbps = excluded.bitrate_kbps,
    size_bytes = excluded.size_bytes, mtime = excluded.mtime,
    scanned_at = excluded.scanned_at
"#;
