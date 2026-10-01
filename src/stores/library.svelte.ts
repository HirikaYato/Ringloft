/**
 * Библиотека: папки, прогресс сканирования, поиск.
 *
 * Поиск идёт в SQLite (FTS5), поэтому результат ограничен сотней строк —
 * этого хватает, чтобы выбрать нужное, и не грузит IPC.
 */
import { convertFileSrc } from '@tauri-apps/api/core';

import {
  api,
  toErrorPayload,
  type AlbumRow,
  type BrowseScope,
  type LibraryStats,
  type LibraryTrack,
  type NamedCount,
  type ScanProgress,
  type SmartList,
} from '../lib/api';
import { on, type ScanFinished } from '../lib/events';

/** Пауза перед запросом, пока человек ещё печатает. */
const SEARCH_DEBOUNCE = 180;

export type BrowseMode = 'albums' | 'artists' | 'genres' | 'folders' | 'smart' | 'stats';

/**
 * Встроенные умные списки — это те же правила, просто неизменяемые. Держим
 * их во фронте: в базе им делать нечего, а править их незачем.
 */
export function builtinSmartLists(): SmartList[] {
  const dayInSeconds = 86_400;
  const now = Math.floor(Date.now() / 1000);
  return [
    {
      id: -1,
      name: 'Любимое',
      rules: {
        rules: [{ field: 'rating', op: 'greater', value: '3' }],
        sort: 'rating',
        descending: true,
        limit: null,
      },
    },
    {
      id: -2,
      name: 'Ни разу не слушал',
      rules: {
        rules: [{ field: 'playCount', op: 'empty', value: '' }],
        sort: 'added',
        descending: true,
        limit: null,
      },
    },
    {
      id: -3,
      name: 'Часто слушаю',
      rules: {
        rules: [{ field: 'playCount', op: 'greater', value: '4' }],
        sort: 'playCount',
        descending: true,
        limit: 200,
      },
    },
    {
      id: -4,
      name: 'Давно не слушал',
      rules: {
        // Значение — момент времени: «раньше, чем 90 дней назад». Строки без
        // прослушиваний сюда не попадают, у них поле пустое.
        rules: [{ field: 'lastPlayed', op: 'less', value: String(now - 90 * dayInSeconds) }],
        sort: 'lastPlayed',
        descending: false,
        limit: null,
      },
    },
    {
      id: -5,
      name: 'Недавно добавленное',
      rules: {
        rules: [{ field: 'addedAt', op: 'withinDays', value: '30' }],
        sort: 'added',
        descending: true,
        limit: 300,
      },
    },
    {
      id: -6,
      name: 'Случайные 50',
      rules: { rules: [], sort: 'random', descending: false, limit: 50 },
    },
  ];
}

export type Selection = { scope: BrowseScope; value: string; label: string };

class LibraryStore {
  stats = $state<LibraryStats | null>(null);
  folders = $state<string[]>([]);
  scanning = $state(false);
  progress = $state<ScanProgress | null>(null);
  lastScan = $state<ScanFinished | null>(null);

  query = $state('');
  results = $state<LibraryTrack[]>([]);
  searching = $state(false);

  mode = $state<BrowseMode>('albums');
  albums = $state<AlbumRow[]>([]);
  names = $state<NamedCount[]>([]);
  selection = $state<Selection | null>(null);
  /** Открытый умный список: у него нет области обзора, только правила. */
  smart = $state<SmartList | null>(null);
  tracks = $state<LibraryTrack[]>([]);
  /** Ключ альбома → адрес миниатюры. null — обложки нет, повторно не просим. */
  covers = $state<Map<string, string | null>>(new Map());
  private coverRequests = new Set<string>();

  private searchTimer: ReturnType<typeof setTimeout> | null = null;
  private searchSeq = 0;

  /** Открыть умный список: результат показывается там же, где треки альбома. */
  async loadSmart(list: SmartList): Promise<void> {
    try {
      this.smart = list;
      this.selection = null;
      this.albums = [];
      this.tracks = await api.librarySmartTracks(list.rules);
    } catch (err) {
      console.error('[ringloft] умный список', toErrorPayload(err));
    }
  }

  async setRating(path: string, rating: number): Promise<void> {
    try {
      await api.librarySetRating(path, rating);
      // Показанные строки правим на месте: перезапрос ради одной оценки
      // выглядел бы как мигание списка.
      for (const collection of [this.tracks, this.results]) {
        const found = collection.find((track) => track.path === path);
        if (found) found.rating = rating;
      }
    } catch (err) {
      console.error('[ringloft] оценка не записалась', toErrorPayload(err));
    }
  }

  async init(folders: string[]): Promise<void> {
    this.folders = folders;
    await Promise.all([
      on('library:scan-progress', (progress) => {
        this.scanning = progress.phase !== 'cancelled';
        this.progress = progress;
      }),
      on('library:scan-finished', (finished) => {
        this.scanning = false;
        this.progress = null;
        this.lastScan = finished;
        void this.refreshStats();
        // Содержимое разделов после скана устарело, обложки — нет.
        void this.loadMode(this.mode);
        if (this.query) void this.runSearch(this.query);
      }),
    ]);
    this.scanning = await api.libraryScanning().catch(() => false);
    await this.refreshStats();
    await this.loadMode('albums');
  }

  async loadMode(mode: BrowseMode): Promise<void> {
    this.mode = mode;
    this.selection = null;
    this.tracks = [];
    // Итоги грузит их экран сам — у него свой период.
    if (mode === 'stats') {
      this.names = [];
      this.albums = [];
      return;
    }
    try {
      if (mode === 'albums') {
        this.names = [];
        this.albums = await api.libraryAlbums();
      } else {
        this.albums = [];
        this.names =
          mode === 'artists'
            ? await api.libraryArtists()
            : mode === 'genres'
              ? await api.libraryGenres()
              : await api.libraryFolders();
      }
    } catch (err) {
      console.error('[ringloft] раздел библиотеки', toErrorPayload(err));
    }
  }

  /** Клик по исполнителю показывает его альбомы, по жанру и папке — треки. */
  async select(scope: BrowseScope, value: string, label = value): Promise<void> {
    this.selection = { scope, value, label };
    try {
      if (scope === 'artist') {
        this.albums = await api.libraryAlbums(value);
        this.tracks = [];
      } else {
        this.tracks = await api.libraryTracks(scope, value);
      }
    } catch (err) {
      console.error('[ringloft] содержимое раздела', toErrorPayload(err));
    }
  }

  /** Треки раздела без изменения текущего выбора — для кнопки «играть». */
  async tracksOf(scope: BrowseScope, value: string): Promise<LibraryTrack[]> {
    try {
      return await api.libraryTracks(scope, value);
    } catch (err) {
      console.error('[ringloft] треки раздела', toErrorPayload(err));
      return [];
    }
  }

  back(): void {
    void this.loadMode(this.mode);
  }

  /** Обложка подгружается, когда плитка появилась на экране. */
  async requestCover(albumKey: string): Promise<void> {
    if (this.covers.has(albumKey) || this.coverRequests.has(albumKey)) return;
    this.coverRequests.add(albumKey);
    try {
      const path = await api.libraryAlbumCover(albumKey);
      this.covers = new Map(this.covers).set(albumKey, path ? convertFileSrc(path) : null);
    } catch (err) {
      console.error('[ringloft] обложка альбома', toErrorPayload(err));
      this.covers = new Map(this.covers).set(albumKey, null);
    } finally {
      this.coverRequests.delete(albumKey);
    }
  }

  /** Обложку альбома вписали в файлы — забыть старую и запросить заново.
      Метка в адресе — чтобы вебвью не показал прежнюю картинку из кэша. */
  async refreshCover(albumKey: string): Promise<void> {
    const next = new Map(this.covers);
    next.delete(albumKey);
    this.covers = next;
    await this.requestCover(albumKey);
    const url = this.covers.get(albumKey);
    if (url) this.covers = new Map(this.covers).set(albumKey, `${url}?v=${Date.now()}`);
  }

  setFolders(folders: string[]): void {
    this.folders = folders;
  }

  async refreshStats(): Promise<void> {
    try {
      this.stats = await api.libraryStats();
    } catch (err) {
      console.error('[ringloft] статистика библиотеки', toErrorPayload(err));
    }
  }

  async scan(): Promise<void> {
    this.scanning = await api.libraryScanStart();
  }

  async cancelScan(): Promise<void> {
    await api.libraryScanCancel();
  }

  async addFolders(paths: string[]): Promise<void> {
    const settings = await api.libraryFoldersAdd(paths);
    this.folders = settings.library.folders;
    this.scanning = true;
  }

  async removeFolder(path: string): Promise<void> {
    const settings = await api.libraryFolderRemove(path);
    this.folders = settings.library.folders;
  }

  /** Ввод в поле поиска: ждём паузу и только потом идём в базу. */
  search(query: string): void {
    this.query = query;
    if (this.searchTimer) clearTimeout(this.searchTimer);
    if (!query.trim()) {
      this.results = [];
      this.searching = false;
      return;
    }
    this.searching = true;
    this.searchTimer = setTimeout(() => void this.runSearch(query), SEARCH_DEBOUNCE);
  }

  private async runSearch(query: string): Promise<void> {
    const seq = ++this.searchSeq;
    try {
      const found = await api.librarySearch(query);
      // Пока ждали, человек мог напечатать ещё — старый ответ выбрасываем.
      if (seq !== this.searchSeq) return;
      this.results = found;
    } catch (err) {
      console.error('[ringloft] поиск по библиотеке', toErrorPayload(err));
    } finally {
      if (seq === this.searchSeq) this.searching = false;
    }
  }
}

export const library = new LibraryStore();
