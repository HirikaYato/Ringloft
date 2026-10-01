//! Логи: цветные в stderr при разработке, ротация по дням в `app_log_dir`.
//! Уровень переопределяется переменной окружения `RINGLOFT_LOG` (синтаксис EnvFilter).

use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt};

/// Пока жив гард — жив фоновый писатель лога. Держим его в состоянии Tauri.
pub struct LogGuard(#[allow(dead_code)] WorkerGuard);

pub fn init(log_dir: &Path) -> LogGuard {
    let _ = std::fs::create_dir_all(log_dir);

    let (file_writer, guard) =
        tracing_appender::non_blocking(tracing_appender::rolling::daily(log_dir, "ringloft.log"));

    let filter = EnvFilter::try_from_env("RINGLOFT_LOG")
        // Декодер mp3 пишет предупреждение на каждую рамку с «одолженными»
        // данными (`main_data_begin`) — для mp3, начатого с середины или после
        // перемотки, это норма, а не поломка. Настоящие ошибки он отдаёт
        // наверх, и мы их логируем сами. lofty на каждый mp3 без заголовка
        // Xing пишет «длительность по битрейту» — тоже норма, а не поломка.
        .unwrap_or_else(|_| EnvFilter::new("info,ringloft_lib=debug,symphonia_bundle_mp3=error,lofty=error"));

    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(false).with_writer(std::io::stderr))
        .with(
            fmt::layer()
                .with_ansi(false)
                .with_target(true)
                .with_writer(file_writer),
        );

    // try_init, а не init: повторная инициализация (тесты) не должна паниковать.
    if let Err(err) = registry.try_init() {
        eprintln!("Ringloft: логирование уже настроено: {err}");
    }

    LogGuard(guard)
}
