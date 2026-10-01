//! Нагрузочная проверка движка: трек играет с нулевой громкостью (весь путь
//! звука работает, но из колонок тихо), а потоки-пожиратели занимают все
//! ядра с тем же приоритетом, что у движка. Пропуски кольца видно в логе:
//! «за трек колбэк оставался без данных».
//!
//! `cargo run --release --example stress -- файл.flac [секунд] [потоков на ядро]`

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use ringloft_lib::audio::{AudioCmd, EngineHandle, TrackRef};
use ringloft_lib::settings::Settings;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("warn,ringloft_lib=info")
        .init();

    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("использование: cargo run --release --example stress -- <файл> [секунд] [потоков на ядро]");
        return;
    };
    let seconds: u64 = args.next().and_then(|arg| arg.parse().ok()).unwrap_or(20);
    let per_core: usize = args.next().and_then(|arg| arg.parse().ok()).unwrap_or(2);

    let mut settings = Settings::default();
    settings.playback.volume = 0.0;
    let (engine, _events, _taps) = EngineHandle::spawn(&settings);
    let _ = engine.send(AudioCmd::SetVolume(0.0));

    let stop = Arc::new(AtomicBool::new(false));
    let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
    let hogs: Vec<_> = (0..cores * per_core)
        .map(|_| {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let mut x = 0u64;
                while !stop.load(Ordering::Relaxed) {
                    x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
                    std::hint::black_box(x);
                }
            })
        })
        .collect();
    println!("пожирателей: {} на {cores} ядрах, {seconds} с", hogs.len());

    let _ = engine.send(AudioCmd::Load {
        track: TrackRef::file(0, PathBuf::from(&path)),
        autoplay: true,
    });
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(250));
    }

    stop.store(true, Ordering::Relaxed);
    for hog in hogs {
        let _ = hog.join();
    }
    // Остановка движка сама отчитывается о пропусках текущего трека.
    let _ = engine.send(AudioCmd::Shutdown);
    std::thread::sleep(Duration::from_millis(300));
    println!("готово");
}
