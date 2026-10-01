//! Прогон аудиодвижка без окна и вебвью:
//!
//! ```sh
//! cargo run --example play -- /путь/к/файлу.flac 6          # играть 6 секунд
//! cargo run --example play -- /путь/к/файлу.flac 6 4500      # и через секунду прыгнуть на 4.5 с
//! ```
//!
//! Нужен, чтобы проверять звук отдельно от UI: видно устройство, позицию,
//! статус и события движка.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ringloft_lib::audio::{AudioCmd, EngineHandle, PlaybackStatus, TrackRef};
use ringloft_lib::settings::Settings;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("RINGLOFT_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,ringloft_lib=debug")),
        )
        .init();

    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("использование: cargo run --example play -- <файл> [секунд]");
        return;
    };
    let seconds: u64 = args.next().and_then(|arg| arg.parse().ok()).unwrap_or(10);
    let seek_to: Option<u64> = args.next().and_then(|arg| arg.parse().ok());

    let settings = Settings::default();
    // Третий элемент — отвод для визуализатора, здесь он не нужен.
    let (engine, events, _taps) = EngineHandle::spawn(&settings);

    if let Err(err) = engine.send(AudioCmd::Load {
        track: TrackRef::file(0, PathBuf::from(&path)),
        autoplay: true,
    }) {
        eprintln!("движок недоступен: {err}");
        return;
    }

    let deadline = Instant::now() + Duration::from_secs(seconds);
    let seek_at = Instant::now() + Duration::from_secs(1);
    let mut seek_done = seek_to.is_none();
    let mut ended = false;
    let mut started = false;
    while Instant::now() < deadline && !ended {
        if !seek_done && Instant::now() >= seek_at {
            seek_done = true;
            if let Some(position) = seek_to {
                println!("--- перемотка на {position} мс ---");
                let _ = engine.send(AudioCmd::Seek(position));
            }
        }
        while let Ok(event) = events.try_recv() {
            println!("событие: {event:?}");
            ended |= matches!(event, ringloft_lib::audio::EngineEvent::Ended);
        }
        let state = engine.snapshot();
        println!(
            "{:?}  {} / {} мс  устройство: {}",
            state.status,
            state.position_ms,
            state.duration_ms,
            state.device.as_deref().unwrap_or("—")
        );
        // Idle без трека — это ошибка загрузки: движок сбросил состояние.
        if state.status == PlaybackStatus::Idle && state.track.is_none() && started {
            break;
        }
        started |= state.track.is_some();
        std::thread::sleep(Duration::from_millis(500));
    }

    let _ = engine.send(AudioCmd::Shutdown);
    std::thread::sleep(Duration::from_millis(200));
    println!("готово");
}
