//! Слежение за папками библиотеки.
//!
//! Файловые менеджеры и загрузчики сыплют десятками событий на один альбом,
//! поэтому события не разбираются поштучно: любое изменение взводит флаг, а
//! отдельный поток через пару секунд тишины запускает обычный инкрементальный
//! скан. Он и так трогает только изменившееся.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;

use super::LibraryStore;
use crate::playlist::is_audio_path;

/// Сколько ждём тишины, прежде чем пересканировать.
const QUIET_PERIOD: Duration = Duration::from_secs(2);

pub struct FolderWatcher {
    /// Держим watcher живым: его Drop отключает слежение.
    _watcher: RecommendedWatcher,
}

/// Поднимает слежение за папками. `None` — платформа не дала watcher.
pub fn spawn(folders: &[PathBuf], store: Weak<LibraryStore>) -> Option<FolderWatcher> {
    if folders.is_empty() {
        return None;
    }

    let dirty = Arc::new(AtomicBool::new(false));
    let last_event = Arc::new(Mutex::new(Instant::now()));

    let handler_dirty = Arc::clone(&dirty);
    let handler_time = Arc::clone(&last_event);
    let mut watcher = match notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else { return };
        if !is_interesting(&event.kind) || !event.paths.iter().any(|path| affects_library(path)) {
            return;
        }
        *handler_time.lock() = Instant::now();
        handler_dirty.store(true, Ordering::Relaxed);
    }) {
        Ok(watcher) => watcher,
        Err(err) => {
            tracing::warn!(%err, "слежение за папками недоступно");
            return None;
        }
    };

    let mut watched = 0;
    for folder in folders {
        match watcher.watch(folder, RecursiveMode::Recursive) {
            Ok(()) => watched += 1,
            Err(err) => tracing::warn!(%err, path = %folder.display(), "папка не отслеживается"),
        }
    }
    if watched == 0 {
        return None;
    }

    let folders = folders.to_vec();
    let spawned = std::thread::Builder::new()
        .name("ringloft-watch".into())
        .spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(500));
                let Some(store) = store.upgrade() else { break };

                if !dirty.load(Ordering::Relaxed) {
                    continue;
                }
                if last_event.lock().elapsed() < QUIET_PERIOD {
                    continue;
                }
                dirty.store(false, Ordering::Relaxed);

                if store.is_scanning() {
                    // Скан уже идёт — он подберёт изменения сам.
                    continue;
                }
                tracing::debug!("папки изменились, запускаю досканирование");
                store.start_scan(folders.clone());
            }
            tracing::debug!("слежение за папками остановлено");
        });

    if let Err(err) = spawned {
        tracing::error!(%err, "не удалось запустить поток слежения");
        return None;
    }

    tracing::info!(folders = watched, "слежу за папками библиотеки");
    Some(FolderWatcher { _watcher: watcher })
}

fn is_interesting(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}

/// Нас интересуют только аудиофайлы и события на самих папках
/// (переименование каталога приходит без имени файла).
fn affects_library(path: &Path) -> bool {
    is_audio_path(path) || path.extension().is_none()
}
