//! Прогон сканера библиотеки без окна:
//!
//! ```sh
//! cargo run --release --example scan -- ~/Music [строка_поиска] [путь_к_базе]
//! ```
//!
//! Если путь к базе указан, она переиспользуется между запусками — так
//! проверяется инкрементальность: второй проход не должен трогать ничего.
//!
//! Печатает прогресс, время и итоговую статистику — так удобно мерить
//! скорость на больших коллекциях, не открывая приложение.

use std::path::PathBuf;
use std::time::Instant;

use ringloft_lib::library::{LibraryEvent, LibraryStore, ScanPhase};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("RINGLOFT_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let mut args = std::env::args().skip(1);
    let Some(folder) = args.next() else {
        eprintln!("использование: cargo run --example scan -- <папка> [поиск]");
        return;
    };
    let search = args.next().filter(|value| value != "-");
    let reuse_db = args.next().map(PathBuf::from);

    let db_path = reuse_db.unwrap_or_else(|| {
        let mut path = std::env::temp_dir();
        path.push(format!("ringloft-scan-bench-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        path
    });

    let mut cache_dir = std::env::temp_dir();
    cache_dir.push(format!("ringloft-scan-covers-{}", std::process::id()));

    let (library, events) = match LibraryStore::open(db_path.clone(), cache_dir) {
        Ok(pair) => pair,
        Err(err) => {
            eprintln!("не удалось открыть базу: {err}");
            return;
        }
    };

    let started = Instant::now();
    if !library.start_scan(vec![PathBuf::from(&folder)]) {
        eprintln!("сканирование не запустилось");
        return;
    }

    while let Ok(event) = events.recv() {
        match event {
            LibraryEvent::Progress(progress) => {
                let phase = match progress.phase {
                    ScanPhase::Walking => "обход",
                    ScanPhase::Reading => "теги",
                    ScanPhase::Cleaning => "очистка",
                    ScanPhase::Cancelled => "отменено",
                };
                println!(
                    "  [{phase}] {} / {}  {:.1} с",
                    progress.processed,
                    progress.total,
                    started.elapsed().as_secs_f32()
                );
            }
            LibraryEvent::Finished {
                added,
                updated,
                removed,
            } => {
                println!(
                    "готово за {:.2} с: добавлено {added}, обновлено {updated}, удалено {removed}",
                    started.elapsed().as_secs_f32()
                );
                break;
            }
            LibraryEvent::Failed(err) => {
                eprintln!("ошибка: {} {}", err.code, err.message);
                break;
            }
        }
    }

    match library.stats() {
        Ok(stats) => println!(
            "в базе: {} треков, {} альбомов, {} исполнителей, {:.1} ч, {:.1} МБ",
            stats.tracks,
            stats.albums,
            stats.artists,
            stats.total_duration_ms as f64 / 3_600_000.0,
            stats.total_size_bytes as f64 / 1_048_576.0
        ),
        Err(err) => eprintln!("статистика недоступна: {err}"),
    }

    if let Some(query) = search {
        let started = Instant::now();
        match library.search(&query, 20) {
            Ok(found) => {
                println!(
                    "поиск «{query}»: {} совпадений за {:.2} мс",
                    found.len(),
                    started.elapsed().as_secs_f64() * 1000.0
                );
                for track in found.iter().take(5) {
                    println!(
                        "  {} — {}",
                        track.artist.as_deref().unwrap_or("—"),
                        track.title
                    );
                }
            }
            Err(err) => eprintln!("поиск не сработал: {err}"),
        }
    }

    println!("база: {}", db_path.display());
}
