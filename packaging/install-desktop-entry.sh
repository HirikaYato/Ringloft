#!/bin/bash
# Ярлык для запуска сборки из этого каталога (не установленной через deb/rpm).
#
# Нужен он ровно за одним: на Wayland панель задач берёт иконку окна не у
# самого окна, а из .desktop-файла, который находит по app-id. У нас app-id —
# «ringloft», поэтому и файл должен называться ringloft.desktop, а внутри стоять
# StartupWMClass=ringloft. Без этого окно в панели задач остаётся без логотипа,
# хотя в трее иконка есть: её приложение ставит само.
#
# Иконка кладётся в тему hicolor и указывается **по имени**, а не путём.
# Раньше в ярлыке стоял абсолютный путь в каталог проекта, и после переезда
# проекта иконка в панели задач молча пропадала. Путь к бинарнику остаётся
# абсолютным — иначе нельзя, поэтому после переезда скрипт надо запустить ещё
# раз.
#
# Установленным пакетам это не нужно: их .desktop-файл собирает Tauri, и в
# нём всё это уже есть.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$root/src-tauri/target/release/ringloft"
icons="$root/src-tauri/icons"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
target="$data/applications/ringloft.desktop"
theme="$data/icons/hicolor"

if [ ! -x "$binary" ]; then
  echo "Сначала собери релиз: npm run tauri build" >&2
  exit 1
fi

# Каждый размер — своим файлом: мелкие нарисованы от плотного знака, и
# растягивать крупный вниз панель задач не должна.
install_icon() {
  mkdir -p "$theme/$2/apps"
  cp "$1" "$theme/$2/apps/ringloft.$3"
}
install_icon "$icons/32x32.png" 32x32 png
install_icon "$icons/64x64.png" 64x64 png
install_icon "$icons/128x128.png" 128x128 png
install_icon "$icons/128x128@2x.png" 256x256 png
install_icon "$icons/icon.png" 512x512 png
install_icon "$root/packaging/logo.svg" scalable svg

mkdir -p "$(dirname "$target")"
cat > "$target" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Ringloft
GenericName=Аудиоплеер
Comment=Аудиоплеер для больших локальных коллекций
Exec=$binary %F
Icon=ringloft
Terminal=false
Categories=AudioVideo;Audio;Player;
MimeType=audio/mpeg;audio/flac;audio/x-flac;audio/ogg;audio/x-vorbis+ogg;audio/x-opus+ogg;audio/opus;audio/x-wav;audio/wav;audio/mp4;audio/x-m4a;audio/aac;audio/x-aiff;audio/x-mpegurl;application/x-cue;
StartupNotify=true
StartupWMClass=ringloft
DESKTOP

command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$theme" 2>/dev/null || true
command -v update-desktop-database >/dev/null && update-desktop-database "$(dirname "$target")" 2>/dev/null || true
# KDE держит свой кэш ярлыков и иконок: без пересборки он покажет старое до
# следующего входа в сеанс.
command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 >/dev/null 2>&1 || true
# Кэш иконок — отдельно от ярлыков: уже запущенные программы (панель задач в
# plasmashell) помнят, что иконки «ringloft» не было, если окно открывали до
# установки. Тот же сигнал KDE шлёт при смене темы значков — все перечитывают.
command -v gdbus >/dev/null && gdbus emit --session --object-path /KIconLoader \
  --signal org.kde.KIconLoader.iconChanged 0 >/dev/null 2>&1 || true
echo "Готово: $target"
