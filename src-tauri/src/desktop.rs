//! Десктопная обвязка приложения: плагины окна, вторая копия, командная
//! строка, трей. На телефоне ничего из этого нет — там `mobile.rs` с теми же
//! четырьмя входами, и `lib.rs` зовёт их, не зная, на чём запущен.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{Manager, Wry};
use tauri_plugin_window_state::StateFlags;

use crate::media_controls::{self, MediaAction};
use crate::{audio, settings, state, tray};

pub fn plugins(builder: tauri::Builder<Wry>) -> tauri::Builder<Wry> {
    builder
        // single-instance должен идти первым: он решает, жить ли этому процессу.
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            on_second_instance(app, argv, cwd);
        }))
        // Рамку окна рисуем сами, и восстанавливать её плагину нельзя:
        // сохранённое состояние прошлых версий (`decorated: true`) иначе
        // возвращает системный заголовок поверх нашего.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::all() & !StateFlags::DECORATIONS)
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
}

pub fn setup(app: &tauri::AppHandle, settings: &Arc<settings::SettingsStore>) {
    if let Err(err) = tray::build(app) {
        tracing::error!(%err, "иконка в трее не создана");
    }
    setup_window(app, Arc::clone(settings));
}

/// Существующие пути из командной строки: и файлы, и папки.
/// Флаги вроде `--flag` игнорируем, чтобы не спорить с аргументами Tauri.
pub fn launch_paths() -> Vec<PathBuf> {
    std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .filter(|path| !path.to_string_lossy().starts_with('-') && path.exists())
        .collect()
}

/// `ringloft --play-pause` при незапущенном плеере: действие выполняется
/// сразу после восстановления сессии.
pub fn launch_action() -> Option<MediaAction> {
    let argv: Vec<String> = std::env::args().collect();
    cli_action(&argv)
}

/// Команды управления из командной строки. Нужны там, где системные
/// медиаклавиши до плеера не доходят: в KDE такую строку можно повесить на
/// любое сочетание клавиш, и она дойдёт до уже запущенной копии.
fn cli_action(argv: &[String]) -> Option<MediaAction> {
    argv.iter().skip(1).find_map(|arg| match arg.as_str() {
        "--play-pause" => Some(MediaAction::Toggle),
        "--play" => Some(MediaAction::Play),
        "--pause" => Some(MediaAction::Pause),
        "--stop" => Some(MediaAction::Stop),
        "--next" => Some(MediaAction::Next),
        "--prev" | "--previous" => Some(MediaAction::Previous),
        _ => None,
    })
}

/// Вторая копия приложения передаёт сюда свои аргументы и умирает.
fn on_second_instance(app: &tauri::AppHandle, argv: Vec<String>, cwd: String) {
    if let Some(action) = cli_action(&argv) {
        // Окно наверх не поднимаем: это управление воспроизведением, а не
        // просьба показать плеер.
        if let Some(state) = app.try_state::<state::AppState>() {
            let context = media_controls::MediaContext {
                engine: state.engine.clone(),
                playlist: Arc::clone(&state.playlist),
                settings: Arc::clone(&state.settings),
                app: app.clone(),
            };
            media_controls::handle_action(&context, action);
        }
        return;
    }

    media_controls::show_window(app);

    // Относительный путь — относительно каталога второй копии, а не нашего:
    // `ringloft трек.flac` из терминала иначе искал бы файл не там.
    let base = PathBuf::from(cwd);
    let paths: Vec<PathBuf> = argv
        .into_iter()
        .skip(1)
        .filter(|arg| !arg.starts_with('-'))
        .map(|arg| {
            let path = PathBuf::from(&arg);
            if path.is_relative() && !audio::is_stream(&path) {
                base.join(path)
            } else {
                path
            }
        })
        .filter(|path| audio::is_stream(path) || path.exists())
        .collect();
    if paths.is_empty() {
        return;
    }

    let Some(state) = app.try_state::<state::AppState>() else {
        return;
    };
    if let Err(err) = state.controls().open_paths(&paths) {
        tracing::error!(%err, "не удалось включить переданные файлы");
    }
}

/// События окна: закрытие в трей (если так настроено) и присмотр за тем,
/// чтобы окно не вылезало за рабочую область экрана.
fn setup_window(app: &tauri::AppHandle, settings: Arc<settings::SettingsStore>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    clamp_to_work_area(&window);

    let hidden = window.clone();
    window.on_window_event(move |event| match event {
        tauri::WindowEvent::CloseRequested { api, .. } if settings.get().ui.minimize_to_tray => {
            api.prevent_close();
            let _ = hidden.hide();
        }
        tauri::WindowEvent::Resized(_) => clamp_to_work_area(&hidden),
        _ => {}
    });
}

/// Размер окна восстанавливается из файла состояния, а экран с тех пор мог
/// смениться — или размер был сохранён вместе с системной рамкой, которой у
/// нас больше нет. Тогда окно выше экрана, и нижняя панель плеера уезжает за
/// край: на Wayland подвинуть себя окно не может, положение назначает
/// композитор.
///
/// Чинить обрезкой на пиксель нельзя: панели рабочего стола в `work_area` на
/// Wayland не попадают, и окно ровно во всю высоту экрана всё равно не
/// поместится. Поэтому негодный размер заменяем на размер по умолчанию.
fn clamp_to_work_area(window: &tauri::WebviewWindow) {
    if window.is_maximized().unwrap_or(false) || window.is_fullscreen().unwrap_or(false) {
        return;
    }
    let Ok(Some(monitor)) = window.current_monitor() else {
        return;
    };
    let area = monitor.work_area().size;
    let Ok(size) = window.outer_size() else {
        return;
    };
    if size.width < area.width && size.height < area.height {
        return;
    }

    let width = DEFAULT_WINDOW.0.min(area.width * 9 / 10);
    let height = DEFAULT_WINDOW.1.min(area.height * 9 / 10);
    tracing::info!(
        from = format!("{}x{}", size.width, size.height),
        to = format!("{width}x{height}"),
        "сохранённый размер окна не помещается на экран"
    );
    let _ = window.set_size(tauri::PhysicalSize::new(width, height));
}

/// Тот же размер, что в `tauri.conf.json`: к нему возвращаемся, когда
/// сохранённый не годится.
const DEFAULT_WINDOW: (u32, u32) = (1180, 720);
