/**
 * Доступ к окну Tauri. Вынесено в обёртку, потому что `getCurrentWindow()`
 * падает, если фронт открыт не в вебвью (например, в обычном браузере при
 * отладке вёрстки). Остальному коду достаточно проверить `null`.
 */
import { LogicalSize, type PhysicalSize } from '@tauri-apps/api/dpi';
import { getCurrentWindow, type Window } from '@tauri-apps/api/window';

import { api } from './api';

export function appWindow(): Window | null {
  try {
    return getCurrentWindow();
  } catch {
    return null;
  }
}

/** Размер компактного окна и нижняя граница для него. */
const MINI = new LogicalSize(430, 84);
const MINI_MIN = new LogicalSize(320, 76);
/** Та же нижняя граница, что в `tauri.conf.json` для обычного режима. */
const NORMAL_MIN = new LogicalSize(680, 420);

/** Размер окна до перехода в компактный режим — чтобы вернуть тот же. */
let beforeMini: PhysicalSize | null = null;

/**
 * Компактный режим — это не только другая вёрстка, но и другое окно:
 * минимальный размер из конфига пришлось бы обходить в любом случае.
 */
export async function applyMiniWindow(mini: boolean): Promise<void> {
  const win = appWindow();
  if (!win) return;
  try {
    if (mini) {
      beforeMini = await win.innerSize();
      await win.setMinSize(MINI_MIN);
      await win.setSize(MINI);
      return;
    }
    await win.setMinSize(NORMAL_MIN);
    await win.setSize(beforeMini ?? new LogicalSize(1180, 720));
    beforeMini = null;
  } catch (err) {
    console.warn('[ringloft] не удалось перестроить окно', err);
  }
}

/**
 * Окно поверх остальных: у компактного режима это главный сценарий.
 *
 * Делает это бэкенд, а не `setAlwaysOnTop` напрямую: на Wayland приложение
 * само поднять себя не может, и просьбу приходится передавать композитору.
 * `false` — не получилось, и об этом стоит сказать вслух.
 */
export async function applyAlwaysOnTop(on: boolean): Promise<boolean> {
  if (!appWindow()) return false;
  try {
    return await api.windowAlwaysOnTop(on);
  } catch (err) {
    console.warn('[ringloft] не удалось закрепить окно', err);
    return false;
  }
}
