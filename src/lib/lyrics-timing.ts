/**
 * Где мы в синхронном тексте. Строки уже отсортированы по метке (это делает
 * бэкенд), поэтому поиск двоичный. Пустая строка с меткой — это пауза в
 * песне: на ней подсвечивать нечего, и прошлая строка гаснет.
 */
import type { LyricLine } from '../bindings/LyricLine';

/** Последняя строка, чья метка уже прошла; -1 — текст ещё не начался. */
export function activeLine(lines: LyricLine[], positionMs: number): number {
  let low = 0;
  let high = lines.length - 1;
  let found = -1;
  while (low <= high) {
    const middle = (low + high) >> 1;
    const at = lines[middle]?.atMs ?? 0;
    if (at <= positionMs) {
      found = middle;
      low = middle + 1;
    } else {
      high = middle - 1;
    }
  }
  return found;
}

/** Метка следующей строки после `index`; `null` — дальше строк нет. */
export function nextStamp(lines: LyricLine[], index: number): number | null {
  return lines[index + 1]?.atMs ?? null;
}

/** Пауза, ради которой стоит показать точки, а не пустое место. */
export const LONG_PAUSE_MS = 4_000;

/** Сколько длится строка `index`: до следующей метки. */
export function lineLength(lines: LyricLine[], index: number, durationMs: number): number {
  const at = index < 0 ? 0 : (lines[index]?.atMs ?? 0);
  const next = nextStamp(lines, index) ?? durationMs;
  return Math.max(0, next - at);
}
