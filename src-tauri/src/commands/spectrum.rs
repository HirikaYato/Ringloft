use tauri::State;
use tauri::ipc::Channel;

use crate::state::AppState;

/// Фронт открывает канал, когда визуализатор виден, и закрывает, когда нет:
/// иначе мы считали бы БПФ и гоняли кадры впустую.
#[tauri::command]
pub fn spectrum_subscribe(state: State<'_, AppState>, channel: Channel<Vec<f32>>) {
    state.spectrum.subscribe(channel);
}

#[tauri::command]
pub fn spectrum_unsubscribe(state: State<'_, AppState>) {
    state.spectrum.unsubscribe();
}
