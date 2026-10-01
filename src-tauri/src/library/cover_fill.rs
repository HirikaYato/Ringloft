//! Обложки из интернета для альбомов библиотеки: какие файлы у альбома,
//! есть ли уже картинка и пакетный проход по всем альбомам без неё.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::AppHandle;
use ts_rs::TS;

use super::LibraryStore;
use crate::cover_fetch::{self, CoverFetchResult};
use crate::error::{RingloftError, Result};

pub const EVENT_COVERS_PROGRESS: &str = "covers:progress";
pub const EVENT_COVERS_FINISHED: &str = "covers:finished";

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "CoversProgress.ts")]
pub struct CoversProgress {
    pub processed: u32,
    pub total: u32,
    /// «Исполнитель — Альбом», над которым работаем.
    pub current: String,
    pub found: u32,
}

#[derive(Debug, Clone, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "CoversFinished.ts")]
pub struct CoversFinished {
    pub found: u32,
    pub missing: u32,
    pub failed: u32,
    pub cancelled: bool,
}

impl LibraryStore {
    /// Исполнитель, название и файлы альбома.
    fn album_files(&self, album_key: &str) -> Result<Option<(String, String, Vec<String>)>> {
        self.db.with_read(|connection| {
            let mut statement = connection.prepare(
                "SELECT COALESCE(NULLIF(album_artist, ''), artist, ''), album, path FROM tracks
                 WHERE album_key = ?1 ORDER BY disc_no, track_no",
            )?;
            let mut rows = statement.query([album_key])?;
            let mut found: Option<(String, String, Vec<String>)> = None;
            while let Some(row) = rows.next()? {
                let path: String = row.get(2)?;
                match found.as_mut() {
                    Some((_, _, paths)) => paths.push(path),
                    None => {
                        let album: Option<String> = row.get(1)?;
                        found = Some((row.get(0)?, album.unwrap_or_default(), vec![path]));
                    }
                }
            }
            Ok(found)
        })
    }

    /// Найти обложку альбома и вписать её во все его файлы. Сам по себе не
    /// проверяет, есть ли обложка, — это просьба пользователя.
    pub fn fetch_album_cover(&self, album_key: &str) -> Result<CoverFetchResult> {
        let (artist, album, paths) = self
            .album_files(album_key)?
            .ok_or_else(|| RingloftError::Internal("альбом не найден в библиотеке".into()))?;
        let Some(found) = cover_fetch::find(&artist, &album)? else {
            return Ok(CoverFetchResult::default());
        };
        let result = cover_fetch::embed(&paths, &found);
        self.covers.forget(album_key);
        tracing::info!(artist, album, source = found.source, written = result.written, "обложка вписана");
        Ok(result)
    }

    /// Ключ альбома по файлу — для обложки из нижней панели.
    pub fn album_key_of(&self, path: &str) -> Result<Option<String>> {
        self.db.with_read(|connection| {
            let key = connection
                .query_row("SELECT album_key FROM tracks WHERE path = ?1", [path], |row| {
                    row.get::<_, Option<String>>(0)
                })
                .ok()
                .flatten();
            Ok(key)
        })
    }

    fn album_keys(&self) -> Result<Vec<String>> {
        self.db.with_read(|connection| {
            let mut statement = connection.prepare(
                "SELECT DISTINCT album_key FROM tracks
                 WHERE album_key IS NOT NULL AND album IS NOT NULL AND album != ''",
            )?;
            let keys = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(keys)
        })
    }
}

/// Пакетный поиск обложек — фоновая задача с полосой хода.
pub struct CoverScan {
    running: AtomicBool,
    cancel: AtomicBool,
}

impl CoverScan {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            running: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
        })
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    /// Все альбомы без обложки. `false` — задача уже идёт.
    pub fn start(self: &Arc<Self>, app: AppHandle, library: Arc<LibraryStore>) -> bool {
        if self.running.swap(true, Ordering::SeqCst) {
            return false;
        }
        self.cancel.store(false, Ordering::SeqCst);
        let scan = Arc::clone(self);
        let spawned = std::thread::Builder::new()
            .name("ringloft-covers".into())
            .spawn(move || {
                let report = scan.run(&app, &library);
                scan.running.store(false, Ordering::SeqCst);
                tracing::info!(found = report.found, missing = report.missing, "поиск обложек закончен");
                crate::events::emit(&app, EVENT_COVERS_FINISHED, report);
            });
        if let Err(err) = spawned {
            tracing::error!(%err, "не удалось запустить поиск обложек");
            self.running.store(false, Ordering::SeqCst);
            return false;
        }
        true
    }

    fn run(&self, app: &AppHandle, library: &LibraryStore) -> CoversFinished {
        let mut report = CoversFinished::default();
        let keys = match library.album_keys() {
            Ok(keys) => keys,
            Err(err) => {
                tracing::error!(%err, "альбомы для обложек не прочитались");
                return report;
            }
        };
        // Сначала — какие альбомы без картинки: это локальная проверка
        // (тег или cover.jpg рядом), в сеть идём только за остальными.
        let missing: Vec<String> = keys
            .into_iter()
            .filter(|key| matches!(library.covers.ensure(&library.db, key), Ok(None)))
            .collect();
        let total = missing.len() as u32;
        for (index, key) in missing.iter().enumerate() {
            if self.cancel.load(Ordering::SeqCst) {
                report.cancelled = true;
                break;
            }
            let current = library
                .album_files(key)
                .ok()
                .flatten()
                .map(|(artist, album, _)| if artist.is_empty() { album } else { format!("{artist} — {album}") })
                .unwrap_or_default();
            crate::events::emit(
                app,
                EVENT_COVERS_PROGRESS,
                CoversProgress {
                    processed: index as u32,
                    total,
                    current,
                    found: report.found,
                },
            );
            match library.fetch_album_cover(key) {
                Ok(result) if result.found => report.found += 1,
                Ok(_) => report.missing += 1,
                Err(err) => {
                    tracing::debug!(%err, "обложка не нашлась");
                    report.failed += 1;
                }
            }
        }
        report
    }
}
