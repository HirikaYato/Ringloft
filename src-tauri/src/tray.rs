//! Иконка в трее: меню управления и быстрый доступ к окну.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::audio::AudioCmd;
use crate::error::{RingloftError, Result};
use crate::media_controls::show_window;
use crate::state::AppState;

pub const TRAY_ID: &str = "ringloft";

pub fn build(app: &AppHandle) -> Result<()> {
    let toggle = MenuItem::with_id(app, "toggle", "Пуск / пауза", true, None::<&str>)?;
    let prev = MenuItem::with_id(app, "prev", "Предыдущий", true, None::<&str>)?;
    let next = MenuItem::with_id(app, "next", "Следующий", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "Показать окно", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;

    let menu = Menu::with_items(
        app,
        &[&toggle, &prev, &next, &separator, &show, &separator, &quit],
    )?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| RingloftError::Internal("у приложения нет иконки для трея".into()))?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip("Ringloft")
        .menu(&menu)
        // Левый клик показывает окно, меню — по правому: так ведут себя плееры.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_window(tray.app_handle());
            }
        })
        .build(app)?;

    tracing::info!("иконка в трее создана");
    Ok(())
}

/// Подсказка трея показывает, что играет.
pub fn set_tooltip(app: &AppHandle, text: &str) {
    if let Some(tray) = app.tray_by_id(TRAY_ID)
        && let Err(err) = tray.set_tooltip(Some(text))
    {
        tracing::debug!(%err, "подсказка трея не обновилась");
    }
}

fn on_menu(app: &AppHandle, id: &str) {
    if id == "quit" {
        // Настройки и плейлист сохранит обработчик RunEvent::Exit.
        app.exit(0);
        return;
    }
    if id == "show" {
        show_window(app);
        return;
    }

    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let result = match id {
        "toggle" => state.engine.send(AudioCmd::TogglePause),
        "prev" => state.controls().prev(),
        "next" => state.controls().next(true),
        _ => Ok(()),
    };
    if let Err(err) = result {
        tracing::warn!(%err, id, "пункт меню трея не сработал");
    }
}
