/**
 * Плейлист во фронте — это окно в список, который живёт в Rust.
 *
 * Целиком 50 000 строк через IPC не гоняем: бэкенд отдаёт срез, а список
 * рисует только видимые строки. Кэшируем одно окно с запасом сверху и снизу,
 * при выходе за него — перезапрашиваем.
 */
import {
  api,
  toErrorPayload,
  type PlaylistRow,
  type PlaylistSort,
  type PlaylistTab,
  type SortKey,
  type SourcesRefresh,
} from '../lib/api';
import { on } from '../lib/events';
import { plural } from '../lib/format';
import { notice } from './notice.svelte.ts';

/** Запас строк за пределами экрана, чтобы прокрутка не упиралась в пустоту. */
const OVERSCAN = 120;

class PlaylistStore {
  count = $state(0);
  currentIndex = $state<number | null>(null);
  sort = $state<PlaylistSort>({ key: 'manual', ascending: true });
  totalDurationMs = $state(0);
  tabs = $state<PlaylistTab[]>([]);
  queued = $state(0);
  queueRows = $state<PlaylistRow[]>([]);

  rows = $state<PlaylistRow[]>([]);
  windowStart = $state(0);
  loading = $state(false);

  /** Поиск по списку: пустая строка — обычный режим. */
  query = $state('');
  found = $state<PlaylistRow[]>([]);
  foundTotal = $state(0);
  /** Порядок воспроизведения вперёд — при перемешивании его больше негде взять. */
  upcoming = $state<PlaylistRow[]>([]);

  /** Сколько строк поиска забираем за раз. */
  private static readonly FOUND_LIMIT = 500;
  private searchTimer: ReturnType<typeof setTimeout> | null = null;

  /** Просьба навести курсор в поиск: счётчик, за ним следит сам список. */
  searchFocus = $state(0);

  /** Выделение храним индексами: с ними работают shift-клик и стрелки. */
  selected = $state<Set<number>>(new Set());
  anchor = $state<number | null>(null);

  private requestedStart = -1;
  private requestedEnd = -1;

  async init(): Promise<void> {
    await on('playlist:changed', () => void this.refresh());
    await this.refresh();
  }

  /**
   * Поиск с задержкой: на каждое нажатие бэкенд перебирает весь список,
   * а пользователь всё равно печатает дальше.
   */
  setQuery(query: string): void {
    this.query = query;
    if (this.searchTimer !== null) clearTimeout(this.searchTimer);
    if (query.trim() === '') {
      this.found = [];
      this.foundTotal = 0;
      return;
    }
    this.searchTimer = setTimeout(() => void this.runSearch(query), 180);
  }

  private async runSearch(query: string): Promise<void> {
    try {
      const result = await api.playlistSearch(query, 0, PlaylistStore.FOUND_LIMIT);
      // Пока ходили за ответом, запрос мог смениться.
      if (this.query !== query) return;
      this.found = result.rows;
      this.foundTotal = result.total;
    } catch (err) {
      console.warn('[ringloft] поиск не удался', toErrorPayload(err));
    }
  }

  async loadUpcoming(): Promise<void> {
    try {
      this.upcoming = await api.playlistUpcoming(200);
    } catch (err) {
      console.warn('[ringloft] порядок воспроизведения не прочитался', toErrorPayload(err));
    }
  }

  row(index: number): PlaylistRow | undefined {
    return this.rows[index - this.windowStart];
  }

  async refresh(): Promise<void> {
    try {
      const meta = await api.playlistMeta();
      this.count = meta.count;
      this.currentIndex = meta.currentIndex;
      this.sort = meta.sort;
      this.totalDurationMs = meta.totalDurationMs;
      // Временная вкладка («Открытые файлы») всегда в конце ленты, отдельно
      // от собранных списков. Сортируем здесь, а не при отрисовке: по этому
      // же порядку работают Alt+цифра и Ctrl+Tab.
      this.tabs = [
        ...meta.tabs.filter((tab) => !tab.isTemporary),
        ...meta.tabs.filter((tab) => tab.isTemporary),
      ];
      this.queued = meta.queued;
      if (this.queued === 0) this.queueRows = [];
      if (this.query.trim() !== '') void this.runSearch(this.query);
      if (this.upcoming.length > 0) void this.loadUpcoming();
      // Окно перечитываем всегда: строки могли уехать или обзавестись тегами.
      await this.fetchWindow(this.windowStart, this.windowStart + this.rows.length, true);
    } catch (err) {
      console.error('[ringloft] плейлист не обновился', toErrorPayload(err));
    }
  }

  /** Вызывается при прокрутке: гарантирует, что диапазон загружен. */
  async ensureRange(first: number, last: number): Promise<void> {
    if (first >= this.requestedStart && last <= this.requestedEnd) return;
    await this.fetchWindow(first, last, false);
  }

  private async fetchWindow(first: number, last: number, force: boolean): Promise<void> {
    const start = Math.max(0, first - OVERSCAN);
    const end = Math.min(this.count, Math.max(last + OVERSCAN, start + 1));
    if (!force && start === this.requestedStart && end === this.requestedEnd) return;

    this.requestedStart = start;
    this.requestedEnd = end;
    this.loading = true;
    try {
      const rows = await api.playlistRows(start, end - start);
      // Пока ждали ответ, могли уехать дальше — тогда результат уже неактуален.
      if (this.requestedStart !== start || this.requestedEnd !== end) return;
      this.windowStart = start;
      this.rows = rows;
    } catch (err) {
      console.error('[ringloft] строки не загрузились', toErrorPayload(err));
    } finally {
      this.loading = false;
    }
  }

  async add(paths: string[], at?: number): Promise<number> {
    try {
      return await api.playlistAdd(paths, at);
    } catch (err) {
      console.error('[ringloft] файлы не добавились', toErrorPayload(err));
      return 0;
    }
  }

  async removeSelected(): Promise<void> {
    const ids = [...this.selected]
      .map((index) => this.row(index)?.id)
      .filter((id): id is number => id !== undefined);
    if (ids.length === 0) return;
    this.selected = new Set();
    await api.playlistRemove(ids);
  }

  async clear(): Promise<void> {
    this.selected = new Set();
    await api.playlistClear();
  }

  async sortBy(key: SortKey): Promise<void> {
    const ascending = this.sort.key === key ? !this.sort.ascending : true;
    await api.playlistSort(key, ascending);
  }

  async move(toIndex: number): Promise<void> {
    const ids = [...this.selected]
      .sort((a, b) => a - b)
      .map((index) => this.row(index)?.id)
      .filter((id): id is number => id !== undefined);
    if (ids.length === 0) return;
    this.selected = new Set();
    await api.playlistMove(ids, toIndex);
  }

  async play(index: number): Promise<void> {
    await api.playlistPlayIndex(index);
  }

  // --- вкладки ---

  focusSearch(): void {
    this.searchFocus += 1;
  }

  async createTab(): Promise<void> {
    await api.playlistCreateTab();
    this.selected = new Set();
  }

  async activateTab(id: number): Promise<void> {
    if (this.tabs.find((tab) => tab.id === id)?.isActive) return;
    this.selected = new Set();
    this.anchor = null;
    // Окно строк относится к другой вкладке — перечитываем с начала.
    this.rows = [];
    this.windowStart = 0;
    this.requestedStart = -1;
    this.requestedEnd = -1;
    await api.playlistActivateTab(id);
  }

  /**
   * Перенести выделенное в другую вкладку. `copy` — оставить и здесь.
   * Индексы, а не пути: строки уезжают целиком, вместе с тегами и границами
   * куска CUE.
   */
  async transferSelected(tabId: number, copy = false): Promise<number> {
    const indexes = [...this.selected].sort((first, second) => first - second);
    if (indexes.length === 0) return 0;
    try {
      const moved = await api.playlistTransfer(indexes, tabId, copy);
      if (moved > 0 && !copy) {
        // Строк больше нет в этой вкладке, выделение указывает в пустоту.
        this.selected = new Set();
        this.anchor = null;
      }
      return moved;
    } catch (err) {
      console.error('[ringloft] перенос строк', toErrorPayload(err));
      return 0;
    }
  }

  /** Убирает строки, файлов которых больше нет. */
  async dropMissing(): Promise<void> {
    try {
      const dropped = await api.playlistDropMissing();
      notice.show(
        dropped > 0
          ? `Убрано ${dropped} ${plural(dropped, 'строка', 'строки', 'строк')} без файла`
          : 'Все файлы на месте',
      );
    } catch (err) {
      console.error('[ringloft] проверка пропавших файлов', toErrorPayload(err));
    }
  }

  /** Вкладка, чьи папки-источники сейчас правят в диалоге. */
  sourcesFor = $state<number | null>(null);
  refreshing = $state(false);

  /** Обновить список по его папкам (без `id` — все списки). */
  async refreshSources(id?: number): Promise<void> {
    if (this.refreshing) return;
    this.refreshing = true;
    try {
      notice.show(describeRefresh(await api.playlistRefreshSources(id)));
    } catch (err) {
      console.error('[ringloft] обновление по папкам', toErrorPayload(err));
    } finally {
      this.refreshing = false;
    }
  }

  async moveTab(id: number, before: number | null): Promise<void> {
    try {
      await api.playlistMoveTab(id, before);
    } catch (err) {
      console.error('[ringloft] порядок вкладок', toErrorPayload(err));
    }
  }

  /** Соседняя вкладка по кругу. */
  async stepTab(delta: number): Promise<void> {
    if (this.tabs.length < 2) return;
    const current = this.tabs.findIndex((tab) => tab.isActive);
    const next = this.tabs[(current + delta + this.tabs.length) % this.tabs.length];
    if (next) await this.activateTab(next.id);
  }

  /** Вкладка по порядковому номеру — для Alt+цифра. */
  async activateByNumber(position: number): Promise<void> {
    const tab = this.tabs[position - 1];
    if (tab) await this.activateTab(tab.id);
  }

  /**
   * Вкладка, которую просят закрыть, — ждёт подтверждения. Закрытие не
   * отменить, а вкладка бывает на сотни строк: крестик у ярлыка убрали из-за
   * промахов, и Ctrl+W с пунктом меню не должны закрывать молча.
   */
  closeRequest = $state<PlaylistTab | null>(null);

  /** Пустую вкладку закрываем сразу, непустую — после подтверждения. */
  requestClose(id: number): void {
    if (this.tabs.length < 2) return;
    const tab = this.tabs.find((item) => item.id === id);
    if (!tab) return;
    if (tab.count === 0) {
      void this.closeTab(id);
      return;
    }
    this.closeRequest = tab;
  }

  async confirmClose(): Promise<void> {
    const tab = this.closeRequest;
    this.closeRequest = null;
    if (tab) await this.closeTab(tab.id);
  }

  /** Последнюю вкладку не закрываем: списку надо где-то жить. */
  closeActiveTab(): void {
    const active = this.tabs.find((tab) => tab.isActive);
    if (active) this.requestClose(active.id);
  }

  async renameTab(id: number, name: string): Promise<void> {
    const trimmed = name.trim();
    if (trimmed) await api.playlistRenameTab(id, trimmed);
  }

  async closeTab(id: number): Promise<void> {
    await api.playlistCloseTab(id);
  }

  // --- M3U ---

  async importList(path: string): Promise<number> {
    try {
      return await api.playlistImport(path);
    } catch (err) {
      console.error('[ringloft] импорт списка', toErrorPayload(err));
      return 0;
    }
  }

  async exportM3u(path: string): Promise<number> {
    try {
      return await api.playlistExportM3u(path);
    } catch (err) {
      console.error('[ringloft] экспорт списка', toErrorPayload(err));
      return 0;
    }
  }

  // --- очередь ---

  async enqueueSelected(): Promise<void> {
    const ids = [...this.selected]
      .sort((a, b) => a - b)
      .map((index) => this.row(index)?.id)
      .filter((id): id is number => id !== undefined);
    if (ids.length === 0) return;
    await api.playlistEnqueue(ids);
  }

  async loadQueue(): Promise<void> {
    try {
      this.queueRows = await api.playlistQueueRows();
    } catch (err) {
      console.error('[ringloft] очередь не загрузилась', toErrorPayload(err));
    }
  }

  async clearQueue(): Promise<void> {
    await api.playlistQueueClear();
    this.queueRows = [];
  }

  select(index: number, modifiers: { ctrl: boolean; shift: boolean }): void {
    const next = new Set(modifiers.ctrl ? this.selected : []);
    if (modifiers.shift && this.anchor !== null) {
      const [from, to] = this.anchor < index ? [this.anchor, index] : [index, this.anchor];
      for (let i = from; i <= to; i += 1) next.add(i);
    } else {
      if (modifiers.ctrl && next.has(index)) next.delete(index);
      else next.add(index);
      this.anchor = index;
    }
    this.selected = next;
  }

  selectAll(): void {
    this.selected = new Set(Array.from({ length: this.count }, (_, index) => index));
  }
}

export const playlist = new PlaylistStore();

/** Итог обновления словами: что добавилось, что ушло, до чего не достали. */
export function describeRefresh(report: SourcesRefresh): string {
  const parts: string[] = [];
  if (report.added > 0) parts.push(`добавлено ${report.added}`);
  if (report.removed > 0) parts.push(`убрано ${report.removed}`);
  let text = parts.length > 0 ? `Обновлено по папкам: ${parts.join(', ')}` : 'По папкам ничего нового';
  if (report.unreachable.length > 0) {
    text += `. Недоступно: ${report.unreachable.join(', ')}`;
  }
  return text;
}
