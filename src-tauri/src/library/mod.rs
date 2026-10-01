//! Библиотека: SQLite с полнотекстовым поиском и фоновый сканер папок.

mod browse;
mod cover_fill;
mod covers;
mod maintenance;
mod db;
mod models;
mod queries;
mod scanner;
mod smart;
mod stats;
mod watcher;

pub use browse::{AlbumRow, NamedCount, TrackFilter};
pub use cover_fill::{CoverScan, CoversFinished, CoversProgress};
pub use maintenance::DuplicateGroup;
pub use smart::{SmartField, SmartOp, SmartRule, SmartRules, SmartSort};
pub use models::{LibraryStats, LibraryTrack};
pub use stats::{ListeningStats, PlayRecord, StatsPeriod};

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam_channel::{Receiver, Sender};
use serde::Serialize;
use ts_rs::TS;

use crate::error::{ErrorPayload, Result};
use covers::CoverCache;
use db::Db;
use parking_lot::Mutex;
use watcher::FolderWatcher;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "ScanPhase.ts")]
pub enum ScanPhase {
    /// Обходим папки и считаем файлы.
    Walking,
    /// Читаем теги.
    Reading,
    /// Убираем из базы то, чего больше нет на диске.
    Cleaning,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "ScanProgress.ts")]
pub struct ScanProgress {
    pub phase: ScanPhase,
    pub processed: u32,
    /// 0, пока общее число файлов ещё неизвестно (фаза обхода).
    pub total: u32,
    pub current: Option<String>,
}

impl ScanProgress {
    fn phase(phase: ScanPhase) -> Self {
        Self {
            phase,
            processed: 0,
            total: 0,
            current: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum LibraryEvent {
    Progress(ScanProgress),
    Finished {
        added: u32,
        updated: u32,
        removed: u32,
    },
    Failed(ErrorPayload),
}

/// Минимальное процентное кодирование: в пути кэша бывают пробелы и кириллица,
/// а MPRIS ждёт корректный URL.
pub(crate) fn file_url(path: &std::path::Path) -> String {
    let mut url = String::from("file://");
    for byte in path.to_string_lossy().as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                url.push(*byte as char)
            }
            other => url.push_str(&format!("%{other:02X}")),
        }
    }
    url
}

pub struct LibraryStore {
    db: Arc<Db>,
    covers: CoverCache,
    events: Sender<LibraryEvent>,
    cancel: Arc<AtomicBool>,
    scanning: Arc<AtomicBool>,
    /// Пока жив watcher, живёт и слежение за папками.
    watcher: Mutex<Option<FolderWatcher>>,
}

impl LibraryStore {
    pub fn open(db_path: PathBuf, cache_dir: PathBuf) -> Result<(Arc<Self>, Receiver<LibraryEvent>)> {
        let db = Arc::new(Db::open(db_path)?);
        let (events, events_rx) = crossbeam_channel::unbounded();

        Ok((
            Arc::new(Self {
                db,
                covers: CoverCache::new(cache_dir),
                events,
                cancel: Arc::new(AtomicBool::new(false)),
                scanning: Arc::new(AtomicBool::new(false)),
                watcher: Mutex::new(None),
            }),
            events_rx,
        ))
    }

    /// Перезапускает слежение за папками (после изменения их списка).
    pub fn watch(self: &Arc<Self>, folders: &[PathBuf]) {
        let watcher = watcher::spawn(folders, Arc::downgrade(self));
        *self.watcher.lock() = watcher;
    }

    pub fn artists(&self) -> Result<Vec<NamedCount>> {
        self.db.with_read(browse::artists)
    }

    pub fn genres(&self) -> Result<Vec<NamedCount>> {
        self.db.with_read(browse::genres)
    }

    pub fn folders(&self) -> Result<Vec<NamedCount>> {
        self.db.with_read(browse::folders)
    }

    pub fn albums(&self, artist: Option<&str>) -> Result<Vec<AlbumRow>> {
        self.db.with_read(|connection| browse::albums(connection, artist))
    }

    pub fn tracks_of(&self, filter: &TrackFilter) -> Result<Vec<LibraryTrack>> {
        self.db
            .with_read(|connection| browse::tracks_of(connection, filter))
    }

    /// Обложка конкретного трека как `file://`-адрес: в таком виде её ждёт
    /// MPRIS. Тянет из тега, иначе из картинки рядом с файлом.
    /// Путь к миниатюре обложки трека — его просит системное уведомление:
    /// ему нужен файл, а не `file://`-адрес для вебвью.
    pub fn track_cover_path(&self, track_path: &std::path::Path) -> Option<std::path::PathBuf> {
        self.covers.ensure_for_track(track_path)
    }

    /// Путь к миниатюре обложки; при первом обращении вынимает её из тега.
    pub fn album_cover(&self, album_key: &str) -> Result<Option<String>> {
        Ok(self
            .covers
            .ensure(&self.db, album_key)?
            .map(|path| path.to_string_lossy().into_owned()))
    }

    /// Убирает из базы треки удалённой из библиотеки папки.
    pub fn forget_folder(&self, folder: &std::path::Path) -> Result<u32> {
        let prefix = format!("{}%", folder.to_string_lossy());
        self.db.with_write(|connection| {
            let removed = connection.execute("DELETE FROM tracks WHERE path LIKE ?1", [prefix])?;
            Ok(removed as u32)
        })
    }

    pub fn is_scanning(&self) -> bool {
        self.scanning.load(Ordering::Relaxed)
    }

    /// `false` — сканирование уже идёт, второй раз не запускаем.
    pub fn start_scan(self: &Arc<Self>, folders: Vec<PathBuf>) -> bool {
        if folders.is_empty() {
            return false;
        }
        if self.scanning.swap(true, Ordering::SeqCst) {
            return false;
        }
        self.cancel.store(false, Ordering::SeqCst);

        let store = Arc::clone(self);
        let spawned = std::thread::Builder::new()
            .name("ringloft-scan".into())
            .spawn(move || {
                scanner::run(&store.db, &folders, &store.cancel, &store.events);
                store.scanning.store(false, Ordering::SeqCst);
            });

        if let Err(err) = spawned {
            tracing::error!(%err, "не удалось запустить сканирование");
            self.scanning.store(false, Ordering::SeqCst);
            return false;
        }
        true
    }

    pub fn cancel_scan(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    pub fn stats(&self) -> Result<LibraryStats> {
        self.db.with_read(queries::stats)
    }

    /// Проверка коллекции: похожие треки и пропавшие файлы. Обе — только
    /// показать; убирает строки пользователь, командой `forget`.
    pub fn duplicates(&self, limit: usize) -> Result<Vec<DuplicateGroup>> {
        self.db
            .with_read(|connection| maintenance::duplicates(connection, limit))
    }

    pub fn missing(&self, limit: usize) -> Result<Vec<LibraryTrack>> {
        self.db
            .with_read(|connection| maintenance::missing(connection, limit))
    }

    /// Убирает строки из библиотеки. Файлы не трогает.
    pub fn forget(&self, paths: &[String]) -> Result<u32> {
        self.db
            .with_write(|connection| maintenance::forget(connection, paths))
    }

    /// Пути всех треков — для пакетных задач.
    pub fn all_paths(&self, limit: u32) -> Result<Vec<String>> {
        self.db
            .with_read(|connection| queries::all_paths(connection, limit))
    }

    pub fn search(&self, query: &str, limit: u32) -> Result<Vec<LibraryTrack>> {
        self.db
            .with_read(|connection| queries::search(connection, query, limit.clamp(1, 500)))
    }

    /// Трек дослушали — засчитываем. Файл мог играться не из библиотеки,
    /// тогда просто нечего обновлять.
    /// Прослушивания и оценка по путям — для колонок плейлиста. Файлы вне
    /// библиотеки в ответ не попадают; ошибка базы — просто пустые колонки.
    pub fn track_marks(&self, paths: &[&str]) -> std::collections::HashMap<String, (u32, u32)> {
        self.db
            .with_read(|connection| queries::track_marks(connection, paths))
            .unwrap_or_else(|err| {
                tracing::debug!(%err, "прослушивания для плейлиста не прочитались");
                std::collections::HashMap::new()
            })
    }

    /// Засчитанное прослушивание: в историю (итоги) и в счётчик трека.
    pub fn record_play(&self, play: &PlayRecord, at_unix: u64) {
        match self.db.with_write(|connection| stats::record(connection, play, at_unix)) {
            Ok(()) => tracing::debug!(path = play.path, "прослушивание засчитано"),
            Err(err) => tracing::debug!(%err, "прослушивание не записалось"),
        }
    }

    pub fn listening_stats(&self, period: StatsPeriod, now_unix: u64) -> Result<ListeningStats> {
        self.db
            .with_read(|connection| stats::listening(connection, period, now_unix))
    }

    pub fn set_rating(&self, path: &Path, rating: u32) -> Result<bool> {
        let text = path.to_string_lossy().into_owned();
        self.db
            .with_write(|connection| queries::set_rating(connection, &text, rating))
    }

    /// Треки по условиям умного списка.
    pub fn smart_tracks(&self, rules: &smart::SmartRules) -> Result<Vec<LibraryTrack>> {
        self.db.with_read(|connection| smart::tracks(connection, rules))
    }
}
