//! Спектр в терминале — проверка пути «колбэк → отвод → анализатор» без окна:
//!
//! ```sh
//! cargo run --release --example spectrum -- файл.flac 5
//! ```

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ringloft_lib::audio::{Analyzer, AudioCmd, EngineHandle, TrackRef};
use ringloft_lib::settings::Settings;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("использование: cargo run --example spectrum -- <файл> [секунд]");
        return;
    };
    let seconds: u64 = args.next().and_then(|value| value.parse().ok()).unwrap_or(5);

    let settings = Settings::default();
    let (engine, _events, taps) = EngineHandle::spawn(&settings);
    engine.set_analysis(true);

    if engine
        .send(AudioCmd::Load {
            track: TrackRef::file(0, PathBuf::from(&path)),
            autoplay: true,
        })
        .is_err()
    {
        eprintln!("движок недоступен");
        return;
    }

    // Отвод приезжает, когда движок откроет устройство.
    let Ok(mut consumer) = taps.recv_timeout(Duration::from_secs(5)) else {
        eprintln!("устройство не открылось");
        return;
    };
    println!(
        "отвод: {} Гц, {} каналов",
        consumer.sample_rate, consumer.channels
    );

    let mut analyzer = Analyzer::new(consumer.sample_rate, 24);
    let mut buffer = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(seconds);

    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(200));
        buffer.clear();
        consumer.drain(&mut buffer);
        if !buffer.is_empty() {
            analyzer.feed(&buffer, consumer.channels);
        }

        let levels = analyzer.compute();
        let bars: String = levels
            .iter()
            .map(|level| match (level * 8.0) as usize {
                0 => ' ',
                1 => '▁',
                2 => '▂',
                3 => '▃',
                4 => '▄',
                5 => '▅',
                6 => '▆',
                7 => '▇',
                _ => '█',
            })
            .collect();
        println!("|{bars}|  сэмплов за кадр: {}", buffer.len());
    }

    let _ = engine.send(AudioCmd::Shutdown);
}
