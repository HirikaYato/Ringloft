//! Просьбы к KWin через D-Bus.
//!
//! На Wayland приложение не может ни поднять себя поверх остальных окон, ни
//! показать системное меню окна: протокола для первого нет вовсе, а для
//! второго композитору нужен серийный номер настоящего ввода, которого у
//! команды из вебвью нет. Зато у KDE есть публичные интерфейсы, через которые
//! это делает сам KWin.
//!
//! Всё здесь — «лучшее усилие»: не KDE, не Wayland, KWin занят — значит
//! просто `false`, а плеер работает как работал.

use std::path::Path;

use zbus::blocking::Connection;

const KWIN: &str = "org.kde.KWin";
const GLOBAL_ACCEL: &str = "org.kde.kglobalaccel";

/// Сеанс Wayland: на X11 те же вещи приложение делает само.
pub fn is_wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE").is_ok_and(|kind| kind == "wayland")
}

/// Меню окна — то самое, что KDE показывает по Alt+F3. Открывается для
/// активного окна, а правый клик по нашей полосе как раз делает его активным.
pub fn show_window_menu() -> bool {
    let Ok(connection) = Connection::session() else {
        return false;
    };
    connection
        .call_method(
            Some(GLOBAL_ACCEL),
            "/component/kwin",
            Some("org.kde.kglobalaccel.Component"),
            "invokeShortcut",
            &("Window Operations Menu"),
        )
        .inspect_err(|err| tracing::debug!(%err, "KWin не показал меню окна"))
        .is_ok()
}

/// «Поверх остальных окон» силами KWin: свойство выставляется скриптом,
/// потому что другого пути с Wayland у приложения нет.
pub fn set_keep_above(on: bool, script_path: &Path) -> bool {
    let script = format!(
        "workspace.windowList().forEach(function (w) {{\n\
         \x20 if (w.resourceClass && String(w.resourceClass).indexOf('ringloft') >= 0) {{\n\
         \x20   w.keepAbove = {on};\n\
         \x20 }}\n\
         }});\n"
    );
    if let Err(err) = std::fs::write(script_path, script) {
        tracing::debug!(%err, "скрипт для KWin не записался");
        return false;
    }

    let Ok(connection) = Connection::session() else {
        return false;
    };
    let name = "ringloftKeepAbove";
    // Прошлый прогон мог остаться загруженным — KWin не любит повторных имён.
    let _ = call_scripting(&connection, "unloadScript", &(name));

    let Ok(reply) = call_scripting(
        &connection,
        "loadScript",
        &(script_path.to_string_lossy().to_string(), name.to_owned()),
    ) else {
        return false;
    };
    let Ok(id) = reply.body().deserialize::<i32>() else {
        return false;
    };

    let ran = connection
        .call_method(
            Some(KWIN),
            format!("/Scripting/Script{id}").as_str(),
            Some("org.kde.kwin.Script"),
            "run",
            &(),
        )
        .inspect_err(|err| tracing::debug!(%err, "скрипт KWin не запустился"))
        .is_ok();

    let _ = call_scripting(&connection, "unloadScript", &(name));
    ran
}

fn call_scripting<B>(
    connection: &Connection,
    method: &str,
    body: &B,
) -> zbus::Result<zbus::Message>
where
    B: serde::ser::Serialize + zbus::zvariant::DynamicType,
{
    connection.call_method(
        Some(KWIN),
        "/Scripting",
        Some("org.kde.kwin.Scripting"),
        method,
        body,
    )
}
