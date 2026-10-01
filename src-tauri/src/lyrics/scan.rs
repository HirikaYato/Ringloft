//! Пакетная загрузка текстов: пройти по списку файлов и положить рядом с
//! каждым `.lrc`.
//!
//! Важно не быть свиньёй по отношению к чужому бесплатному серверу: запросы
//! идут **по одному** и с паузой, а файлы, у которых `.lrc` уже лежит,
//! пропускаются без запроса вовсе.
//!
//! Текст **в теге** поводом пропустить не считается: синхронный `.lrc` его
//! перебивает, а сплошная простыня из тега — ровно то, от чего хочется
//! уйти. Если в базе окажется такая же несинхронная простыня, файл не
//! появится (`fetch` это решает сам) и трек попадёт в «уже было».

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::AppHandle;
use ts_rs::TS;

use super::{LyricsFetchStatus, fetch};
use crate::events::{EVENT_LYRICS_FINISHED, EVENT_LYRICS_PROGRESS, emit};

/// Пауза между запросами. У lrclib нет ключей и лимитов на бумаге, но триста
/// миллисекунд — та вежливость, за которую нас не забанят.
const PAUSE: std::time::Duration = std::time::Duration::from_millis(300);

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "LyricsProgress.ts")]
pub struct LyricsProgress {
    pub processed: u32,
    pub total: u32,
    /// Имя файла: в строке прогресса всё равно виден только хвост.
    pub current: Option<String>,
    pub found: u32,
}

#[derive(Debug, Clone, Copy, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "LyricsFinished.ts")]
pub struct LyricsFinished {
    pub found: u32,
    /// Текст уже был — в файле рядом или в теге.
    pub skipped: u32,
    /// В базе нет текста (или это инструментал).
    pub missing: u32,
    pub failed: u32,
    pub cancelled: bool,
}

pub struct LyricsScan {
    running: AtomicBool,
    cancel: AtomicBool,
}

impl LyricsScan {
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

    /// `false` — задача уже идёт или искать нечего.
    pub fn start(self: &Arc<Self>, app: AppHandle, paths: Vec<PathBuf>) -> bool {
        if paths.is_empty() {
            return false;
        }
        if self.running.swap(true, Ordering::SeqCst) {
            return false;
        }
        self.cancel.store(false, Ordering::SeqCst);

        let scan = Arc::clone(self);
        let spawned = std::thread::Builder::new()
            .name("ringloft-lyrics".into())
            .spawn(move || {
                let report = run(paths, &scan.cancel, &mut |progress| {
                    emit(&app, EVENT_LYRICS_PROGRESS, progress);
                });
                scan.running.store(false, Ordering::SeqCst);
                tracing::info!(
                    found = report.found,
                    skipped = report.skipped,
                    missing = report.missing,
                    failed = report.failed,
                    cancelled = report.cancelled,
                    "загрузка текстов закончена"
                );
                emit(&app, EVENT_LYRICS_FINISHED, report);
            });

        if let Err(err) = spawned {
            tracing::error!(%err, "не удалось запустить загрузку текстов");
            self.running.store(false, Ordering::SeqCst);
            return false;
        }
        true
    }
}

/// Прогресс уходит замыканием, а не через `AppHandle`: так работу можно
/// прогнать в тесте, где Tauri нет вовсе.
fn run(
    paths: Vec<PathBuf>,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(LyricsProgress),
) -> LyricsFinished {
    let mut report = LyricsFinished::default();

    // Дубли (треки одного образа CUE) и потоки отсеиваем сразу, а файлы с
    // готовым текстом — до запроса: на тысяче треков это разница между
    // минутой и часом.
    let mut seen = std::collections::HashSet::new();
    let queue: Vec<PathBuf> = paths
        .into_iter()
        .filter(|path| !crate::audio::is_stream(path) && path.is_file())
        .filter(|path| seen.insert(path.clone()))
        .filter(|path| {
            let has = super::sidecar(path).is_file();
            if has {
                report.skipped += 1;
            }
            !has
        })
        .collect();

    let total = queue.len() as u32;
    for (index, path) in queue.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }

        progress(LyricsProgress {
            processed: index as u32,
            total,
            current: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            found: report.found,
        });

        match fetch(path) {
            Ok(result) => match result.status {
                // Нашлось, но файл не появился — значит тег и так не хуже.
                LyricsFetchStatus::Found if result.saved => report.found += 1,
                LyricsFetchStatus::Found => report.skipped += 1,
                _ => report.missing += 1,
            },
            Err(err) => {
                report.failed += 1;
                tracing::warn!(path = %path.display(), %err, "текст не загрузился");
            }
        }

        // Пауза после запроса, а не перед: последний файл не задерживаем.
        if index + 1 < queue.len() {
            std::thread::sleep(PAUSE);
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Файлы с готовым `.lrc` и потоки до сети не доходят: тест прогоняется
    /// без интернета именно поэтому.
    #[test]
    fn skips_what_already_has_lyrics() {
        let dir = std::env::temp_dir().join(format!("ringloft-lyrics-scan-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);

        let with_text = dir.join("есть.wav");
        crate::audio::test_wav::sine(&with_text, 44_100, 2, 4_410, 0.4);
        std::fs::write(super::super::sidecar(&with_text), "[00:01.00]раз").expect("текст");

        let mut steps = Vec::new();
        let report = run(
            vec![
                with_text.clone(),
                with_text.clone(),
                PathBuf::from("https://example.org/stream"),
                PathBuf::from("/такого/файла/нет.flac"),
            ],
            &AtomicBool::new(false),
            &mut |progress| steps.push(progress),
        );

        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(report.skipped, 1, "дубль считается один раз");
        assert_eq!((report.found, report.missing, report.failed), (0, 0, 0));
        assert!(steps.is_empty(), "в сеть ходить было незачем");
    }

    /// Отменённая задача не начинает работу.
    #[test]
    fn cancelled_before_start() {
        let dir = std::env::temp_dir().join(format!("ringloft-lyrics-stop-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("Кишлак - Ночь.wav");
        crate::audio::test_wav::sine(&path, 44_100, 2, 4_410, 0.4);

        let report = run(vec![path], &AtomicBool::new(true), &mut |_| {});
        let _ = std::fs::remove_dir_all(&dir);
        assert!(report.cancelled);
        assert_eq!(report.found, 0);
    }
}

