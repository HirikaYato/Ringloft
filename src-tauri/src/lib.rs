//! Ringloft — аудиоплеер для больших локальных коллекций.
//!
//! Точка сборки приложения: плагины, состояние, команды. Вся доменная логика
//! живёт в модулях, здесь только проводка.

// audio и settings публичные: ими пользуется examples/play.rs — прогон движка
// без окна, чтобы проверять звук отдельно от UI.
pub mod audio;
mod atomic_file;
mod commands;
mod cover_fetch;
mod discord;
mod error;
mod events;
mod files;
mod headphones;
mod identify;
#[cfg(target_os = "linux")]
mod kwin;
mod lastfm;
// library публичный ради examples/scan.rs — прогон сканера без окна.
pub mod library;
mod logging;
mod net;
mod lyrics;
mod media_controls;
#[cfg(target_os = "linux")]
mod mpris;
#[cfg(not(target_os = "linux"))]
mod smtc;
mod sleep;
mod playback;
mod pipewire_rates;
mod playlist;
pub mod settings;
mod spectrum;
mod state;
mod replaygain;
mod tag_edit;
mod text;
mod tags;
mod tray;

pub use error::{ErrorPayload, RingloftError, Result};

use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Instant;

use tauri::{Manager, RunEvent};
use tauri_plugin_window_state::StateFlags;

/// Момент старта процесса — по нему считаем, сколько занял холодный запуск.
static STARTED_AT: OnceLock<Instant> = OnceLock::new();

pub fn started_at() -> Instant {
    *STARTED_AT.get_or_init(Instant::now)
}

/// Файл списка воспроизведения — его открываем отдельной вкладкой.
/// Существующие пути из командной строки: и файлы, и папки.
/// Флаги вроде `--flag` игнорируем, чтобы не спорить с аргументами Tauri.
fn path_arguments() -> Vec<std::path::PathBuf> {
    std::env::args_os()
        .skip(1)
        .map(std::path::PathBuf::from)
        .filter(|path| !path.to_string_lossy().starts_with('-') && path.exists())
        .collect()
}

/// Команды управления из командной строки. Нужны там, где системные
/// медиаклавиши до плеера не доходят: в KDE такую строку можно повесить на
/// любое сочетание клавиш, и она дойдёт до уже запущенной копии.
fn cli_action(argv: &[String]) -> Option<media_controls::MediaAction> {
    argv.iter().skip(1).find_map(|arg| match arg.as_str() {
        "--play-pause" => Some(media_controls::MediaAction::Toggle),
        "--play" => Some(media_controls::MediaAction::Play),
        "--pause" => Some(media_controls::MediaAction::Pause),
        "--stop" => Some(media_controls::MediaAction::Stop),
        "--next" => Some(media_controls::MediaAction::Next),
        "--prev" | "--previous" => Some(media_controls::MediaAction::Previous),
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
    let base = std::path::PathBuf::from(cwd);
    let paths: Vec<std::path::PathBuf> = argv
        .into_iter()
        .skip(1)
        .filter(|arg| !arg.starts_with('-'))
        .map(|arg| {
            let path = std::path::PathBuf::from(&arg);
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

/// HWND нужен SMTC на Windows; на остальных системах медиаконтролям он не нужен.
fn window_handle(app: &tauri::AppHandle) -> Option<usize> {
    #[cfg(target_os = "windows")]
    {
        app.get_webview_window("main")
            .and_then(|window| window.hwnd().ok())
            .map(|hwnd| hwnd.0 as usize)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        None
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

/// Что играть при старте: файл из командной строки важнее сохранённой сессии,
/// а сессия восстанавливается на паузе и с той же позиции, где её оставили.
fn restore_session(controls: &playback::Controls<'_>) {
    let engine = controls.engine;
    let playlist = controls.playlist;

    // Плеер запустили двойным щелчком по файлу: играем его, а прошлую сессию
    // не восстанавливаем. Не заиграло ничего (не тот файл) — тогда сессия.
    let arguments = path_arguments();
    if !arguments.is_empty() {
        match controls.open_paths(&arguments) {
            Ok(true) => return,
            Ok(false) => {}
            Err(err) => tracing::error!(%err, "не удалось открыть пути из командной строки"),
        }
    }

    let Some(id) = playlist.current_id() else {
        return;
    };
    let Some(track) = playlist.track_of(id) else {
        return;
    };

    let position_ms = playlist.restored_position_ms();
    let path = track.path.clone();
    tracing::info!(path = %path.display(), position_ms, "восстанавливаю прошлую сессию");
    let _ = engine.send(audio::AudioCmd::Load {
        track,
        autoplay: false,
    });
    // Позиция имеет смысл только у файла: у радио её восстанавливать некуда,
    // а просьба перемотать заставляла бы движок молча проматывать поток.
    if position_ms > 0 && !audio::is_stream(&path) {
        let _ = engine.send(audio::AudioCmd::Seek(position_ms));
    }
}

pub fn run() {
    started_at();
    let builder = tauri::Builder::default()
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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let path = app.path();

            let log_guard = logging::init(&path.app_log_dir()?);
            app.manage(log_guard);

            let settings_path = path.app_config_dir()?.join("settings.json");
            let settings = Arc::new(settings::SettingsStore::load(settings_path));

            tracing::info!(
                version = env!("CARGO_PKG_VERSION"),
                tauri = tauri::VERSION,
                "Ringloft запускается"
            );

            let (engine, engine_events, taps) = audio::EngineHandle::spawn(&settings.get());
            let spectrum = spectrum::spawn(engine.clone(), taps);
            let (playlist, playlist_events) =
                playlist::PlaylistStore::load(path.app_data_dir()?.join("session.json"));
            let (library, library_events) = library::LibraryStore::open(
                path.app_data_dir()?.join("library.sqlite"),
                path.app_cache_dir()?.join("covers"),
            )?;

            let media = media_controls::spawn(
                media_controls::MediaContext {
                    engine: engine.clone(),
                    playlist: Arc::clone(&playlist),
                    settings: Arc::clone(&settings),
                    app: app.handle().clone(),
                },
                window_handle(app.handle()),
            );

            let scrobbler = lastfm::spawn(
                Arc::clone(&settings),
                path.app_data_dir()?.join("lastfm-queue.json"),
            );

            let discord = discord::spawn(Arc::clone(&settings));
            let sleep = Arc::new(sleep::SleepTimer::default());

            if let Err(err) = tray::build(app.handle()) {
                tracing::error!(%err, "иконка в трее не создана");
            }
            setup_window(app.handle(), Arc::clone(&settings));

            events::spawn(
                app.handle().clone(),
                events::Bridge {
                    engine: engine.clone(),
                    playlist: Arc::clone(&playlist),
                    library: Arc::clone(&library),
                    settings: Arc::clone(&settings),
                    media,
                    scrobbler: scrobbler.clone(),
                    discord: discord.clone(),
                    sleep: Arc::clone(&sleep),
                },
                engine_events,
                playlist_events,
                library_events,
            );

            // Инкрементальный скан на старте: если ничего не менялось, он
            // отработает за миллисекунды, зато библиотека всегда актуальна.
            let folders = settings.get().library.folders;
            if !folders.is_empty() {
                library.start_scan(folders.clone());
                if settings.get().library.watch {
                    library.watch(&folders);
                }
            }

            restore_session(&playback::Controls {
                engine: &engine,
                playlist: &playlist,
                settings: &settings,
            });

            // Плейлисты сверяются со своими папками в фоне: на большой
            // коллекции обход диска — это секунды, а окно ждать не должно.
            if settings.get().library.refresh_playlists {
                let playlist = Arc::clone(&playlist);
                let spawned = std::thread::Builder::new()
                    .name("ringloft-sources".into())
                    .spawn(move || {
                        let report = playlist.refresh_all_sources();
                        tracing::info!(
                            added = report.added,
                            removed = report.removed,
                            unreachable = ?report.unreachable,
                            "плейлисты сверены с папками"
                        );
                    });
                if let Err(err) = spawned {
                    tracing::error!(%err, "не удалось запустить сверку плейлистов");
                }
            }

            // `ringloft --play-pause` при незапущенном плеере: восстановили
            // сессию — и сразу выполняем то, о чём просили.
            let argv: Vec<String> = std::env::args().collect();
            if let Some(action) = cli_action(&argv) {
                let context = media_controls::MediaContext {
                    engine: engine.clone(),
                    playlist: Arc::clone(&playlist),
                    settings: Arc::clone(&settings),
                    app: app.handle().clone(),
                };
                media_controls::handle_action(&context, action);
            }

            app.manage(state::AppState {
                settings,
                playlist,
                library,
                engine,
                spectrum,
                scrobbler,
                gain: replaygain::GainScan::new(),
                lyrics: lyrics::LyricsScan::new(),
                covers: library::CoverScan::new(),
                sleep,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::app_info,
            commands::app::app_paths,
            commands::app::window_show_menu,
            commands::app::window_always_on_top,
            commands::app::discord_probe,
            commands::settings::fonts_installed,
            commands::player::sleep_set,
            commands::library::library_listening,
            commands::playlist::playlist_move_tab,
            commands::identify::identify_track,
            commands::library::covers_fetch_album,
            commands::library::covers_fetch_track,
            commands::library::covers_scan_start,
            commands::library::covers_scan_cancel,
            commands::library::covers_scanning,
            commands::identify::identify_genre,
            commands::headphones::headphones_search,
            commands::headphones::headphones_apply,
            commands::headphones::headphones_import,
            commands::headphones::headphones_set_enabled,
            commands::player::player_set_exact_rate,
            commands::player::audio_rate_status,
            commands::player::pipewire_allow_rates,
            commands::player::sleep_state,
            commands::playlist::playlist_sources,
            commands::playlist::playlist_set_sources,
            commands::playlist::playlist_restore_excluded,
            commands::playlist::playlist_refresh_sources,
            commands::settings::settings_get,
            commands::settings::settings_patch,
            commands::player::player_open,
            commands::player::player_play,
            commands::player::player_pause,
            commands::player::player_toggle,
            commands::player::player_stop,
            commands::player::player_seek,
            commands::player::player_set_volume,
            commands::player::player_set_device,
            commands::player::player_set_replay_gain,
            commands::player::player_set_eq,
            commands::player::player_set_crossfade,
            commands::player::player_state,
            commands::player::audio_devices,
            commands::spectrum::spectrum_subscribe,
            commands::spectrum::spectrum_unsubscribe,
            commands::lastfm::lastfm_status,
            commands::lastfm::lastfm_begin_auth,
            commands::lastfm::lastfm_finish_auth,
            commands::lastfm::lastfm_disconnect,
            commands::tags::tags_read,
            commands::tags::tags_write,
            commands::tags::lyrics_read,
            commands::tags::lyrics_fetch,
            commands::tags::lyrics_scan_start,
            commands::tags::lyrics_scan_playlist,
            commands::tags::lyrics_scan_library,
            commands::tags::lyrics_scan_cancel,
            commands::tags::lyrics_scanning,
            commands::tags::gain_scan_start,
            commands::tags::gain_scan_playlist,
            commands::tags::gain_scan_cancel,
            commands::tags::gain_scanning,
            commands::tags::cover_read,
            commands::playlist::playlist_add,
            commands::playlist::playlist_meta,
            commands::playlist::playlist_rows,
            commands::playlist::playlist_paths,
            commands::playlist::playlist_search,
            commands::playlist::playlist_upcoming,
            commands::playlist::playlist_transfer,
            commands::playlist::playlist_drop_missing,
            commands::files::files_to_trash,
            commands::files::files_rename_preview,
            commands::files::files_rename,
            commands::playlist::playlist_remove,
            commands::playlist::playlist_clear,
            commands::playlist::playlist_move,
            commands::playlist::playlist_sort,
            commands::playlist::playlist_import,
            commands::playlist::playlist_export_m3u,
            commands::playlist::playlist_create_tab,
            commands::playlist::playlist_rename_tab,
            commands::playlist::playlist_close_tab,
            commands::playlist::playlist_activate_tab,
            commands::playlist::playlist_enqueue,
            commands::playlist::playlist_queue_rows,
            commands::playlist::playlist_queue_clear,
            commands::playlist::playlist_play_index,
            commands::playlist::player_next,
            commands::playlist::player_prev,
            commands::playlist::player_set_repeat,
            commands::playlist::player_set_shuffle,
            commands::library::library_scan_start,
            commands::library::library_scan_cancel,
            commands::library::library_scanning,
            commands::library::library_stats,
            commands::library::library_search,
            commands::library::library_duplicates,
            commands::library::library_missing,
            commands::library::library_forget,
            commands::library::library_artists,
            commands::library::library_genres,
            commands::library::library_folders,
            commands::library::library_albums,
            commands::library::library_tracks,
            commands::library::library_smart_tracks,
            commands::library::library_set_rating,
            commands::library::library_album_cover,
            commands::library::library_folders_add,
            commands::library::library_folder_remove,
        ]);

    // build + run вместо Builder::run, чтобы поймать RunEvent::Exit
    // и дописать настройки на диск, не дожидаясь дебаунса.
    let app = match builder.build(tauri::generate_context!()) {
        Ok(app) => app,
        Err(err) => {
            eprintln!("Ringloft: не удалось запустить приложение: {err}");
            std::process::exit(1);
        }
    };

    app.run(|handle, event| {
        if matches!(event, RunEvent::Exit)
            && let Some(state) = handle.try_state::<state::AppState>()
        {
            // Позицию забираем до остановки движка, иначе сохраним ноль.
            let position_ms = state.engine.snapshot().position_ms;
            let _ = state.engine.send(audio::AudioCmd::Shutdown);
            if let Err(err) = state.settings.flush() {
                tracing::error!(%err, "настройки не сохранились при выходе");
            }
            if let Err(err) = state.playlist.save_with_position(position_ms) {
                tracing::error!(%err, "плейлист не сохранился при выходе");
            }
        }
    });
}
