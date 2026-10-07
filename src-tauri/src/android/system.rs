//! Разрешения и хранилище Android: спрашиваем у системы через Kotlin
//! (`Permissions.kt`). Ответ пользователя на просьбу сюда не возвращается —
//! фронт перечитывает состояние сам.

use jni::objects::{JObjectArray, JString};

use super::with_native;
use crate::commands::device::PermissionKind;
use crate::folders::FolderEntry;

pub fn files_allowed() -> bool {
    flag("filesAllowed")
}

pub fn notifications_allowed() -> bool {
    flag("notificationsAllowed")
}

/// Не дозвались до Kotlin — считаем, что разрешения нет: лучше лишний раз
/// спросить, чем молча не найти ни одного файла.
fn flag(method: &'static str) -> bool {
    with_native(method, |env, native| {
        env.call_static_method(native, method, "()Z", &[])?.z()
    })
    .unwrap_or(false)
}

pub fn request_permission(kind: PermissionKind) {
    let method = match kind {
        PermissionKind::Files => "requestFiles",
        PermissionKind::Notifications => "requestNotifications",
    };
    with_native(method, |env, native| {
        env.call_static_method(native, method, "()V", &[])?;
        Ok(())
    });
}

/// Значки системных панелей: тёмные на светлой теме плеера, светлые на
/// тёмной. Окно идёт под панели, и система сама его тему не знает.
pub fn set_light_bars(light: bool) {
    with_native("setLightBars", |env, native| {
        let args = [jni::objects::JValue::Bool(u8::from(light))];
        env.call_static_method(native, "setLightBars", "(Z)V", &args)?;
        Ok(())
    });
}

/// Память телефона и карты памяти. Первой идёт встроенная память.
pub fn storage_roots() -> Vec<FolderEntry> {
    let paths = with_native("storageRoots", |env, native| {
        let array: JObjectArray = env
            .call_static_method(native, "storageRoots", "()[Ljava/lang/String;", &[])?
            .l()?
            .into();
        let mut paths = Vec::new();
        for index in 0..env.get_array_length(&array)? {
            let item: JString = env.get_object_array_element(&array, index)?.into();
            paths.push(env.get_string(&item)?.to_string_lossy().into_owned());
        }
        Ok(paths)
    })
    .unwrap_or_default();

    paths
        .into_iter()
        .enumerate()
        .map(|(index, path)| FolderEntry {
            name: if index == 0 {
                "Память телефона".to_owned()
            } else {
                // Карт бывает несколько, и различить их можно только по метке тома.
                let label = path.rsplit('/').next().unwrap_or_default();
                format!("Карта памяти {label}")
            },
            path,
        })
        .collect()
}
