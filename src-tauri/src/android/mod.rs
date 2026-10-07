//! Привязка к Android, которой Tauri сам не делает.
//!
//! Весь обмен с Kotlin идёт через его объект `Native` и **не зависит от
//! Activity**: её система уничтожает, когда захочет, а музыка в это время
//! играет. Штатный мост плагинов Tauri для этого не годится — он исполняет
//! вызовы на Activity и без неё падает с паникой «no available activity».

mod media;
mod system;

use std::sync::{Once, OnceLock};

use jni::objects::{GlobalRef, JClass, JObject};
use jni::{JNIEnv, JavaVM};
use tauri::{AppHandle, Manager, WebviewWindowBuilder};

pub use media::spawn as spawn_media;
pub use system::{files_allowed, notifications_allowed, request_permission, storage_roots};

/// Главное окно — то же, что в `tauri.conf.json`.
const MAIN_WINDOW: &str = "main";

/// Что нужно, чтобы позвать Kotlin с любого потока: виртуальная машина и сам
/// класс `Native`. Класс запоминаем, а не ищем по имени: с потока, созданного
/// в Rust, `FindClass` видит только системные классы.
struct Bridge {
    vm: JavaVM,
    class: GlobalRef,
}

static BRIDGE: OnceLock<Bridge> = OnceLock::new();
static CONTEXT: Once = Once::new();
static APP: OnceLock<AppHandle> = OnceLock::new();

/// Запоминает приложение: по нему потом пересоздаётся окно.
pub fn set_app(app: &AppHandle) {
    let _ = APP.set(app.clone());
}

/// `Native.activityCreated` из Kotlin: система создала Activity.
///
/// Rust-часть запускается один раз на процесс, и окно из конфига Tauri
/// создаёт тоже один раз — для первой Activity. Когда её уничтожили, а
/// приложение осталось играть (см. `ExitRequested` в `lib.rs`), новая Activity
/// открывается пустой: вебвью в неё никто не кладёт. Кладём сами.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_hirikayato_ringloft_Native_activityCreated<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
) {
    // Первый запуск: приложения ещё нет, окно создаст сам Tauri.
    let Some(app) = APP.get() else {
        return;
    };
    // Вызов пришёл с главного потока Android, а вебвью создаётся на нём же:
    // ждать здесь — значит ждать самих себя. Поэтому отдельный поток.
    let spawned = std::thread::Builder::new()
        .name("ringloft-window".into())
        .spawn(move || reopen_window(app));
    if let Err(err) = spawned {
        tracing::error!(%err, "не удалось запустить пересоздание окна");
    }
}

fn reopen_window(app: &AppHandle) {
    // Activity пересоздали, а окно живо (смена настроек экрана) — не наш случай.
    if app.get_webview_window(MAIN_WINDOW).is_some() {
        return;
    }
    let Some(config) = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == MAIN_WINDOW)
    else {
        return;
    };
    let built = WebviewWindowBuilder::from_config(app, config).and_then(|builder| builder.build());
    match built {
        Ok(_) => tracing::info!("окно создано заново для новой Activity"),
        Err(err) => tracing::error!(%err, "окно для новой Activity не создалось"),
    }
}

/// `Native.initContext` из Kotlin. Зовётся до запуска Rust-части приложения
/// и передаёт контекст **приложения**, а не Activity.
///
/// Он же нужен cpal: тот ходит в Java за списком устройств и параметрами
/// вывода, а JavaVM и контекст берёт из крейта `ndk-context`. В приложениях
/// на NativeActivity его заполняет ndk-glue; у Tauri не заполняет никто, и
/// первое же обращение cpal заканчивалось бы паникой в потоке движка.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_hirikayato_ringloft_Native_initContext<'local>(
    env: JNIEnv<'local>,
    class: JClass<'local>,
    context: JObject<'local>,
) {
    // Activity создаётся заново в том же процессе, а заполнить контекст
    // можно только раз: второй вызов у ndk-context — паника.
    CONTEXT.call_once(|| {
        let (Ok(vm), Ok(class), Ok(context)) = (
            env.get_java_vm(),
            env.new_global_ref(&class),
            env.new_global_ref(&context),
        ) else {
            return;
        };
        let raw = context.as_obj().as_raw();
        // Ссылку не отпускаем: контекст нужен, пока жив процесс.
        std::mem::forget(context);
        // SAFETY: указатель на JavaVM действителен до конца процесса, а
        // глобальная ссылка на контекст выше намеренно не освобождается.
        unsafe {
            ndk_context::initialize_android_context(vm.get_java_vm_pointer().cast(), raw.cast());
        }
        let _ = BRIDGE.set(Bridge { vm, class });
    });
}

/// Зовёт статический метод `Native` в Kotlin. `call` получает окружение JNI
/// и класс; всё, что он создаст (строки), освобождается по выходе, поэтому
/// наружу он отдаёт уже значения Rust.
///
/// Ошибка не роняет ничего — вместо ответа приходит `None`: системный плеер
/// не то, из-за чего должна замолчать музыка.
pub(crate) fn with_native<T>(
    what: &'static str,
    call: impl FnOnce(&mut JNIEnv<'_>, &GlobalRef) -> jni::errors::Result<T>,
) -> Option<T> {
    let bridge = BRIDGE.get()?;
    // Поток остаётся привязанным к машине до своего конца: наши потоки
    // живут долго, и привязывать их на каждый вызов незачем.
    let mut env = match bridge.vm.attach_current_thread_permanently() {
        Ok(env) => env,
        Err(err) => {
            tracing::warn!(%err, what, "поток не привязался к Java");
            return None;
        }
    };
    // Поток из Rust в Java не возвращается, поэтому локальные ссылки сами
    // не освобождаются — отсюда свой кадр на каждый вызов.
    match env.with_local_frame(16, |env| call(env, &bridge.class)) {
        Ok(value) => Some(value),
        Err(err) => {
            // Неснятое исключение Java валит следующий же вызов JNI на потоке.
            let _ = env.exception_describe();
            let _ = env.exception_clear();
            tracing::warn!(%err, what, "вызов в Kotlin не удался");
            None
        }
    }
}
