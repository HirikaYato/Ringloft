/**
 * Зеркало серверных настроек. Источник правды — бэкенд: сюда кладётся только
 * то, что он вернул после нормализации, поэтому UI не может выставить
 * недопустимое значение и «залипнуть» на нём.
 */
import {
  api,
  toErrorPayload,
  type DeepPartial,
  type ErrorPayload,
  type RepeatMode,
  type Settings,
} from '../lib/api';
import { FONT_SIZE } from '../lib/typography.svelte.ts';

class SettingsStore {
  current = $state<Settings | null>(null);
  error = $state<ErrorPayload | null>(null);
  loading = $state(true);

  async load(): Promise<void> {
    this.loading = true;
    try {
      this.current = await api.settingsGet();
      this.error = null;
    } catch (err) {
      this.error = toErrorPayload(err);
    } finally {
      this.loading = false;
    }
  }

  /**
   * Повтор и перемешивание идут отдельной командой: их запоминает ещё и
   * играющий плейлист, чтобы при возврате к нему режим восстановился.
   */
  async setRepeat(repeat: RepeatMode): Promise<void> {
    await this.adopt(() => api.playerSetRepeat(repeat));
  }

  /** Порядок обхода повтора один и тот же у кнопки транспорта и у клавиши. */
  async cycleRepeat(): Promise<void> {
    const order: RepeatMode[] = ['off', 'all', 'track'];
    const current = this.current?.playback.repeat ?? 'off';
    await this.setRepeat(order[(order.indexOf(current) + 1) % order.length] ?? 'off');
  }

  async toggleShuffle(): Promise<void> {
    await this.setShuffle(!(this.current?.playback.shuffle ?? false));
  }

  async setShuffle(shuffle: boolean): Promise<void> {
    await this.adopt(() => api.playerSetShuffle(shuffle));
  }

  /** Размер шрифта: и клавишами Ctrl+= / Ctrl+−, и из настроек. */
  async nudgeFontSize(delta: number): Promise<void> {
    const size = this.current?.ui.fontSize ?? FONT_SIZE.default;
    const next = Math.min(FONT_SIZE.max, Math.max(FONT_SIZE.min, size + delta));
    if (next !== size) await this.patch({ ui: { fontSize: next } });
  }

  async resetFontSize(): Promise<void> {
    await this.patch({ ui: { fontSize: FONT_SIZE.default } });
  }

  private async adopt(action: () => Promise<Settings>): Promise<void> {
    try {
      this.current = await action();
      this.error = null;
    } catch (err) {
      this.error = toErrorPayload(err);
    }
  }

  async patch(patch: DeepPartial<Settings>): Promise<void> {
    try {
      this.current = await api.settingsPatch(patch);
      this.error = null;
    } catch (err) {
      this.error = toErrorPayload(err);
    }
  }
}

export const settings = new SettingsStore();

/**
 * Свой цвет акцента поверх темы. Значение приходит уже проверенным с
 * бэкенда — в CSS попадает только `#rrggbb`.
 */
export function applyAccent(color: string | null | undefined): void {
  const root = document.documentElement;
  if (color) root.style.setProperty('--accent', color);
  else root.style.removeProperty('--accent');
}

/** «Как в системе» сводится к светлой или тёмной. */
export function resolvedTheme(theme: Settings['ui']['theme']): 'dark' | 'light' {
  if (theme !== 'system') return theme;
  return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
}

/** Тему держим на <html data-theme>, чтобы CSS-переменные работали без JS. */
export function applyTheme(theme: Settings['ui']['theme']): void {
  document.documentElement.dataset.theme = resolvedTheme(theme);
}
