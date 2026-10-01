//! Свой подсчёт громкости: измеряем файлы по ITU-R BS.1770 и записываем
//! результат в теги ReplayGain.
//!
//! Зачем: в собранной по частям коллекции метки есть у половины файлов, и
//! между треками скачет громкость. Считать в момент воспроизведения нельзя —
//! измерение требует всего трека целиком, поэтому это фоновая задача, а
//! результат живёт в файле и достаётся оттуда при следующем открытии.
//!
//! Альбомное усиление считается по **выбранным** файлам: гейтирование у
//! альбома общее, и набор для него — то, что попало в задачу. Выбрали три
//! трека из двенадцати — альбомная метка будет про эти три, поэтому в один
//! альбом объединяются только файлы, у которых совпал тег альбома, и только
//! когда их больше одного.

mod tags;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::AppHandle;
use ts_rs::TS;

use crate::audio::loudness::{self, Measured};
use crate::events::{EVENT_GAIN_FINISHED, EVENT_GAIN_PROGRESS, emit};

#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "GainPhase.ts")]
pub enum GainPhase {
    /// Декодируем и считаем.
    Measuring,
    /// Записываем метки в файлы.
    Writing,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "GainProgress.ts")]
pub struct GainProgress {
    pub phase: GainPhase,
    pub processed: u32,
    pub total: u32,
    /// Имя файла, а не путь: в строке прогресса всё равно виден только хвост.
    pub current: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "GainFinished.ts")]
pub struct GainFinished {
    pub measured: u32,
    /// Метка уже была, а пересчитывать не просили.
    pub skipped: u32,
    pub failed: u32,
    pub cancelled: bool,
}

pub struct GainScan {
    running: AtomicBool,
    cancel: AtomicBool,
}

impl GainScan {
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

    /// `false` — задача уже идёт или считать нечего.
    pub fn start(self: &Arc<Self>, app: AppHandle, paths: Vec<PathBuf>, force: bool) -> bool {
        if paths.is_empty() {
            return false;
        }
        if self.running.swap(true, Ordering::SeqCst) {
            return false;
        }
        self.cancel.store(false, Ordering::SeqCst);

        let scan = Arc::clone(self);
        let spawned = std::thread::Builder::new()
            .name("ringloft-gain".into())
            .spawn(move || {
                let report = run(paths, force, &scan.cancel, &mut |progress| {
                    emit(&app, EVENT_GAIN_PROGRESS, progress);
                });
                scan.running.store(false, Ordering::SeqCst);
                tracing::info!(
                    measured = report.measured,
                    skipped = report.skipped,
                    failed = report.failed,
                    cancelled = report.cancelled,
                    "подсчёт громкости закончен"
                );
                emit(&app, EVENT_GAIN_FINISHED, report);
            });

        if let Err(err) = spawned {
            tracing::error!(%err, "не удалось запустить подсчёт громкости");
            self.running.store(false, Ordering::SeqCst);
            return false;
        }
        true
    }
}

/// Прогресс уходит наружу замыканием, а не через `AppHandle`: так эту работу
/// можно прогнать в тесте, где Tauri нет вовсе.
fn run(
    paths: Vec<PathBuf>,
    force: bool,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(GainProgress),
) -> GainFinished {
    let mut report = GainFinished::default();
    let mut queue = Vec::new();
    let mut seen = HashSet::new();

    // Потоки измерять нечем, дубли (треки одного образа CUE) считаем один
    // раз, а файлы с готовой меткой пропускаем, если не просили заново.
    for path in paths {
        if crate::audio::is_stream(&path) || !path.is_file() {
            continue;
        }
        if !seen.insert(path.clone()) {
            continue;
        }
        let probe = tags::probe(&path);
        if probe.tagged && !force {
            report.skipped += 1;
            continue;
        }
        queue.push((path, probe.album_key));
    }

    let total = queue.len() as u32;
    let mut done: Vec<(PathBuf, Option<String>, Measured)> = Vec::new();

    for (path, album_key) in queue {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        progress(step(GainPhase::Measuring, done.len() as u32, total, &path));

        match loudness::measure(&path, cancel) {
            Ok(Some(measured)) => done.push((path, album_key, measured)),
            Ok(None) => {
                report.cancelled = true;
                break;
            }
            Err(err) => {
                report.failed += 1;
                tracing::warn!(path = %path.display(), %err, "громкость посчитать не удалось");
            }
        }
    }

    // Альбомные метки — только когда набор полон: по половине альбома
    // считать нечего.
    let albums = if report.cancelled {
        HashMap::new()
    } else {
        album_loudness(&done)
    };

    let total_writes = done.len() as u32;
    for (index, (path, album_key, measured)) in done.iter().enumerate() {
        progress(step(GainPhase::Writing, index as u32, total_writes, path));
        let album = album_key.as_ref().and_then(|key| albums.get(key));

        match tags::write(path, &measured.loudness, album) {
            Ok(()) => report.measured += 1,
            Err(err) => {
                report.failed += 1;
                tracing::warn!(path = %path.display(), %err, "метку записать не удалось");
            }
        }
    }

    report
}

/// Громкость по альбомам: в один альбом попадают файлы с одинаковым тегом,
/// и только если их больше одного.
fn album_loudness(
    done: &[(PathBuf, Option<String>, Measured)],
) -> HashMap<String, loudness::Loudness> {
    let mut groups: HashMap<&String, Vec<&Measured>> = HashMap::new();
    for (_, album_key, measured) in done {
        if let Some(key) = album_key {
            groups.entry(key).or_default().push(measured);
        }
    }

    groups
        .into_iter()
        .filter(|(_, tracks)| tracks.len() > 1)
        .map(|(key, tracks)| (key.clone(), loudness::album_loudness(&tracks)))
        .collect()
}

fn step(
    phase: GainPhase,
    processed: u32,
    total: u32,
    path: &std::path::Path,
) -> GainProgress {
    GainProgress {
        phase,
        processed,
        total,
        current: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::test_wav;
    use crate::tag_edit::TagEdit;
    use crate::tags::read_replay_gain;

    /// Файл с синусом и, если задан альбом, с тегами.
    fn track(name: &str, amplitude: f32, album: Option<&str>) -> PathBuf {
        let path = test_wav::temp_path(name);
        test_wav::sine(&path, 44_100, 2, 44_100 * 6, amplitude);
        if let Some(album) = album {
            let edit = TagEdit {
                album: Some(album.to_owned()),
                artist: Some("Кто-то".to_owned()),
                ..Default::default()
            };
            let _ = crate::tag_edit::write(&path, &edit);
        }
        path
    }

    fn scan(paths: Vec<PathBuf>, force: bool) -> (GainFinished, Vec<GainProgress>) {
        let mut steps = Vec::new();
        let report = run(paths, force, &AtomicBool::new(false), &mut |progress| {
            steps.push(progress)
        });
        (report, steps)
    }

    #[test]
    fn measures_writes_and_skips_already_tagged() {
        let path = track("scan", 0.5, None);
        let (report, steps) = scan(vec![path.clone()], false);

        assert_eq!(report.measured, 1);
        assert_eq!((report.skipped, report.failed, report.cancelled), (0, 0, false));
        // Обе фазы должны быть видны снаружи.
        assert!(steps.iter().any(|step| matches!(step.phase, GainPhase::Measuring)));
        assert!(steps.iter().any(|step| matches!(step.phase, GainPhase::Writing)));

        // -6.02 dBFS синус: усиление до опорных -18 LUFS.
        let written = read_replay_gain(&path);
        let gain = written.track_gain_db.unwrap_or_default();
        assert!((gain + 11.98).abs() < 0.4, "{gain}");
        assert!((written.track_peak.unwrap_or_default() - 0.5).abs() < 0.01);

        // Повторный проход по тому же файлу ничего не считает...
        let (again, _) = scan(vec![path.clone()], false);
        assert_eq!((again.measured, again.skipped), (0, 1));
        // ...а с `force` считает.
        let (forced, _) = scan(vec![path.clone()], true);
        let _ = std::fs::remove_file(&path);
        assert_eq!((forced.measured, forced.skipped), (1, 0));
    }

    /// Один файл в списке дважды (так выглядят треки одного образа CUE) —
    /// это одно измерение.
    #[test]
    fn duplicates_are_measured_once() {
        let path = track("dup", 0.4, None);
        let (report, _) = scan(vec![path.clone(), path.clone(), path.clone()], false);
        let _ = std::fs::remove_file(&path);
        assert_eq!(report.measured, 1);
    }

    #[test]
    fn missing_files_and_streams_are_ignored() {
        let (report, steps) = scan(
            vec![
                PathBuf::from("https://example.org/stream"),
                PathBuf::from("/такого/файла/нет.flac"),
            ],
            false,
        );
        assert_eq!(
            (report.measured, report.skipped, report.failed),
            (0, 0, 0)
        );
        assert!(steps.is_empty());
    }

    /// Альбомная метка появляется только там, где альбом собран из
    /// нескольких файлов: по одному треку считать альбом нечего.
    #[test]
    fn album_tags_need_more_than_one_track() {
        let first = track("album-a", 0.5, Some("Вместе"));
        let second = track("album-b", 0.1, Some("Вместе"));
        let alone = track("album-c", 0.3, Some("Один"));

        let (report, _) = scan(vec![first.clone(), second.clone(), alone.clone()], false);
        assert_eq!(report.measured, 3);

        let loud = read_replay_gain(&first);
        let quiet = read_replay_gain(&second);
        let single = read_replay_gain(&alone);
        for path in [&first, &second, &alone] {
            let _ = std::fs::remove_file(path);
        }

        // У альбома метка общая, а у треков — своя.
        assert_eq!(loud.album_gain_db, quiet.album_gain_db);
        assert!(loud.album_gain_db.is_some());
        assert_ne!(loud.track_gain_db, quiet.track_gain_db);
        assert_eq!(single.album_gain_db, None);
        assert!(single.track_gain_db.is_some());
    }

    /// Отмена не должна оставлять альбомные метки: набор треков уже не полон.
    #[test]
    fn cancelled_scan_writes_nothing() {
        let path = track("cancelled", 0.5, Some("Вместе"));
        let cancel = AtomicBool::new(true);
        let report = run(vec![path.clone()], false, &cancel, &mut |_| {});
        let written = read_replay_gain(&path);
        let _ = std::fs::remove_file(&path);

        assert!(report.cancelled);
        assert_eq!(report.measured, 0);
        assert_eq!(written.track_gain_db, None);
    }
}

