//! Команды Tauri. Слой тонкий: разбор аргументов, вызов доменной логики,
//! преобразование ошибки в `RingloftError`. Никакой логики здесь не живёт.

pub mod app;
pub mod files;
pub mod headphones;
pub mod identify;
pub mod lastfm;
pub mod library;
pub mod player;
pub mod playlist;
pub mod settings;
pub mod spectrum;
pub mod tags;

/// Блокирующая работа (диск, теги, SQLite) — в пул, а не в UI-поток.
pub(crate) async fn blocking<T, F>(task: F) -> crate::error::Result<T>
where
    F: FnOnce() -> crate::error::Result<T> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|err| {
            crate::error::RingloftError::Internal(format!("фоновая задача не выполнилась: {err}"))
        })?
}
