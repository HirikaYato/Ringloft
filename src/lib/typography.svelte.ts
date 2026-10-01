/**
 * Шрифт и его размер. Размер в настройках один — основной; мелкий, крупный
 * и высота строк списков считаются от него здесь, чтобы вёрстка и
 * виртуализация не разъехались: строка рисуется той высоты, на какую
 * рассчитывает прокрутка.
 */

const SYSTEM_STACK = "system-ui, 'Segoe UI', Roboto, 'Noto Sans', sans-serif";

export type FontOption = { id: string; label: string; stack: string };

/** Встроенные шрифты: лежат в сборке (пакеты @fontsource), есть везде. */
export const BUILTIN_FONTS: FontOption[] = [
  { id: 'system', label: 'Системный', stack: SYSTEM_STACK },
  { id: 'inter', label: 'Inter', stack: `'Inter Variable', ${SYSTEM_STACK}` },
  { id: 'golos', label: 'Golos Text', stack: `'Golos Text Variable', ${SYSTEM_STACK}` },
  { id: 'manrope', label: 'Manrope', stack: `'Manrope Variable', ${SYSTEM_STACK}` },
  { id: 'nunito', label: 'Nunito', stack: `'Nunito Variable', ${SYSTEM_STACK}` },
];

export const FONT_SIZE = { min: 12, max: 20, default: 15 } as const;

/** Ключ встроенного шрифта или имя установленного семейства. */
function fontStack(font: string): string {
  const builtin = BUILTIN_FONTS.find((option) => option.id === font);
  // Имя приходит проверенным с бэкенда (буквы, цифры, пробел, `-_.`),
  // поэтому в кавычки его можно ставить как есть.
  return builtin?.stack ?? `'${font}', ${SYSTEM_STACK}`;
}

/** Высоты строк — реактивно: списки перестраиваются при смене размера. */
export const metrics = $state({ row: 30, compact: 28 });

export function applyTypography(size: number, font: string, lyricsFont: string): void {
  const wanted = Number.isFinite(size) ? Math.round(size) : FONT_SIZE.default;
  const md = Math.min(FONT_SIZE.max, Math.max(FONT_SIZE.min, wanted));
  // Только запись, без чтения: функция зовётся из эффекта, и чтение
  // `metrics` подписало бы эффект на то, что он сам меняет, — вечный цикл.
  const row = Math.round(md * 2);
  metrics.row = row;
  metrics.compact = row - 2;

  const style = document.documentElement.style;
  style.setProperty('--fs-xs', `${Math.max(10, md - 3)}px`);
  style.setProperty('--fs-sm', `${md - 2}px`);
  style.setProperty('--fs-md', `${md}px`);
  style.setProperty('--fs-lg', `${md + 2}px`);
  style.setProperty('--row-h', `${row}px`);
  style.setProperty('--row-h-compact', `${row - 2}px`);
  style.setProperty('--chrome-h', `${md + 29}px`);
  style.setProperty('--font-ui', fontStack(font));
  style.setProperty('--font-lyrics', fontStack(lyricsFont));
}
