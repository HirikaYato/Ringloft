//! Привязка к Android, которой Tauri сам не делает.
//!
//! cpal ходит в Java за списком устройств и параметрами вывода, а JavaVM и
//! контекст берёт из крейта `ndk-context`. В приложениях на NativeActivity
//! его заполняет ndk-glue; у Tauri своя Activity, и не заполняет никто —
//! первое же обращение cpal заканчивалось бы паникой в потоке движка.

use std::sync::Once;

use jni::JNIEnv;
use jni::objects::{JClass, JObject};

static CONTEXT: Once = Once::new();

/// `Native.initContext` из Kotlin. Зовётся до запуска Rust-части приложения
/// и передаёт контекст **приложения**, а не Activity: её система может
/// уничтожить, пока музыка играет.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_hirikayato_ringloft_Native_initContext<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    context: JObject<'local>,
) {
    // Activity создаётся заново в том же процессе, а заполнить контекст
    // можно только раз: второй вызов у ndk-context — паника.
    CONTEXT.call_once(|| {
        let (Ok(vm), Ok(context)) = (env.get_java_vm(), env.new_global_ref(&context)) else {
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
    });
}
