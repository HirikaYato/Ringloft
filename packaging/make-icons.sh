#!/usr/bin/env bash
# Собирает иконки приложения из packaging/logo.svg и logo-small.svg.
#
# Крупные размеры берутся от тонкого знака, мелкие (16-48) — от плотного:
# на 32 пикселях тонкие кольца исчезают в сглаживании. Обычный
# `tauri icon` так не умеет — он делает все кадры из одной картинки.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
icons="$root/src-tauri/icons"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

render() { rsvg-convert -w "$2" -h "$2" "$1" -o "$3"; }

echo "== рисую знак =="
render "$root/packaging/logo.svg" 1024 "$work/big-1024.png"
# Где чей вариант: плотный держится до 64 включительно, тонкий хорош от 96
# (сравнивалось на рендерах — у тонкого на 48 кольца рвутся сглаживанием).
for size in 128 256; do render "$root/packaging/logo.svg" "$size" "$work/big-$size.png"; done
for size in 16 24 30 32 44 48 64; do render "$root/packaging/logo-small.svg" "$size" "$work/small-$size.png"; done

echo "== полный набор через tauri icon =="
(cd "$root" && npm run --silent tauri -- icon "$work/big-1024.png")

# Мобильных целей у плеера нет, а tauri icon кладёт наборы и для них.
rm -rf "$icons/android" "$icons/ios"

echo "== мелкие размеры — от плотного знака =="
cp "$work/small-32.png" "$icons/32x32.png"
cp "$work/small-64.png" "$icons/64x64.png"
cp "$work/small-30.png" "$icons/Square30x30Logo.png"
cp "$work/small-44.png" "$icons/Square44x44Logo.png"

echo "== .ico: каждый кадр от своего варианта =="
python3 "$root/packaging/pack-ico.py" "$icons/icon.ico" \
  16 "$work/small-16.png" 24 "$work/small-24.png" 32 "$work/small-32.png" \
  48 "$work/small-48.png" 64 "$work/small-64.png" 128 "$work/big-128.png" \
  256 "$work/big-256.png"

echo "готово: $icons"
