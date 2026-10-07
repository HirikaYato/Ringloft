# Окружение для сборки под Android. Подключается, а не запускается:
#   . scripts/android-env.sh
# (`npm run android -- …` делает это сам.)
#
# Пути по умолчанию — те, куда всё ставится без root: JDK 17 в
# ~/.jdks/temurin-17, SDK в ~/Android/Sdk, NDK внутри него. Своё расположение
# задаётся теми же переменными заранее (JDK — через RINGLOFT_JAVA_HOME).
# В CI этот же скрипт: SDK раннер задаёт сам, NDK называет сборка.

# Gradle 8.14 из шаблона Tauri не запускается на JDK новее 24, поэтому берём
# 17 отдельно от системной и не глядя на общий JAVA_HOME. JAVA_HOME_17_X64 —
# так JDK 17 называется на раннерах GitHub.
export JAVA_HOME="${RINGLOFT_JAVA_HOME:-${JAVA_HOME_17_X64:-$HOME/.jdks/temurin-17}}"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"

if [ -z "${NDK_HOME:-}" ]; then
  # NDK ставятся бок о бок по версиям — берём самую новую.
  NDK_HOME="$(ls -d "$ANDROID_HOME"/ndk/* 2>/dev/null | sort -V | tail -n 1)"
  export NDK_HOME
fi

# libopus собирается CMake из сборочного скрипта opusic-sys, а тот ищет NDK
# только под этим именем; уровень API — тот же, что minSdkVersion в
# tauri.android.conf.json.
export ANDROID_NDK_HOME="$NDK_HOME"
export ANDROID_PLATFORM="${ANDROID_PLATFORM:-android-26}"

# Отладочная сборка под телефон идёт без отладочной информации: с ней
# библиотека весит 470 МБ вместо нескольких десятков, и APK по Wi-Fi ставится
# минутами. Отладчиком на телефоне не ходим, а место паники печатается и так.
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-0}"

for required in "$JAVA_HOME/bin/java" "$ANDROID_HOME/platform-tools/adb" "$NDK_HOME/toolchains"; do
  if [ ! -e "$required" ]; then
    echo "android-env: не найдено $required" >&2
  fi
done
