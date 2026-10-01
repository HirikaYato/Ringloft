/**
 * Состояние плеера на фронте — это зеркало бэкенда. Локально ничего не
 * "додумываем": позиция приходит тиками, статус — событиями, а начальный
 * слепок берём одним player_state при старте.
 */
import {
  api,
  toErrorPayload,
  type AudioDevice,
  type ErrorPayload,
  type Lyrics,
  type LyricsFetchStatus,
  type PlaybackStatus,
  type TrackInfo,
  type TrackTags,
} from '../lib/api';
import { dominantColor } from '../lib/cover-color';
import { on } from '../lib/events';

class PlayerStore {
  status = $state<PlaybackStatus>('idle');
  positionMs = $state(0);
  durationMs = $state(0);
  volume = $state(0.7);
  track = $state<TrackInfo | null>(null);
  device = $state<string | null>(null);
  devices = $state<AudioDevice[]>([]);
  error = $state<ErrorPayload | null>(null);
  tags = $state<TrackTags | null>(null);
  coverUrl = $state<string | null>(null);
  /** Доминирующий цвет обложки: им подкрашивается нижняя панель. */
  coverTint = $state<string | null>(null);
  /** Название от радиостанции: у потока оно меняется без смены трека. */
  streamTitle = $state<string | null>(null);
  /** Текст песни. Читается лениво — только когда панель текста открыта. */
  lyrics = $state<Lyrics | null>(null);
  /** Ход поиска текста в интернете: он идёт по кнопке и не мгновенно. */
  lyricsSearch = $state<'idle' | 'busy' | LyricsFetchStatus>('idle');
  /** Как трек назван в базе текстов и лёг ли файл рядом. */
  lyricsMatched = $state<string | null>(null);
  lyricsSaved = $state(false);
  private lyricsPath: string | null = null;
  /** Когда пришла последняя позиция: от неё идут часы `livePositionMs`. */
  private positionAt = 0;
  /** Громкость до выключения звука: иначе «включить обратно» вернуло бы не то. */
  private beforeMute = 0.7;

  private setPosition(ms: number): void {
    this.positionMs = ms;
    this.positionAt = performance.now();
  }

  /**
   * Позиция «прямо сейчас», а не на момент последнего тика. Тики приходят
   * раз в 66 мс и неровно (IPC, занятый поток вебвью), поэтому то, что
   * должно попадать в такт — строка текста песни, — досчитывает время само.
   * Дольше полсекунды без тиков вперёд не уезжаем: значит, что-то встало.
   */
  livePositionMs(): number {
    if (this.status !== 'playing') return this.positionMs;
    const ahead = Math.min(500, performance.now() - this.positionAt);
    const ms = this.positionMs + ahead;
    return this.durationMs > 0 ? Math.min(ms, this.durationMs) : ms;
  }

  async init(): Promise<void> {
    await Promise.all([
      on('player:tick', (tick) => {
        this.setPosition(tick.positionMs);
        this.status = tick.status;
      }),
      on('player:track', (track) => {
        this.track = track;
        this.streamTitle = null;
        this.durationMs = track.durationMs ?? 0;
        this.setPosition(0);
        this.error = null;
        // Теги читаем отдельным запросом: движку незачем ждать файловый ввод-вывод.
        void this.loadMetadata(track.path);
      }),
      on('player:status', (status) => {
        this.status = status;
      }),
      on('player:stream-title', (title) => {
        this.streamTitle = title;
      }),
      on('player:ended', () => {
        this.setPosition(this.durationMs);
      }),
      on('app:error', (payload) => {
        this.error = payload;
      }),
    ]);

    await this.refresh();
    await this.refreshDevices();
  }

  async refresh(): Promise<void> {
    try {
      const state = await api.playerState();
      this.status = state.status;
      this.setPosition(state.positionMs);
      this.durationMs = state.durationMs;
      this.volume = state.volume;
      this.track = state.track;
      this.device = state.device;
      if (state.track) void this.loadMetadata(state.track.path);
    } catch (err) {
      this.error = toErrorPayload(err);
    }
  }

  async refreshDevices(): Promise<void> {
    try {
      this.devices = await api.audioDevices();
    } catch (err) {
      this.error = toErrorPayload(err);
    }
  }

  async open(path: string): Promise<void> {
    await this.run(() => api.playerOpen(path, true));
  }

  async seek(positionMs: number): Promise<void> {
    this.setPosition(positionMs);
    await this.run(() => api.playerSeek(positionMs));
  }

  /**
   * Что показывать как название: у радио оно приходит из эфира и важнее
   * тегов, у файла — тег, а если и его нет, то строка плейлиста.
   */
  get title(): string {
    return this.streamTitle || this.tags?.title || this.track?.title || '';
  }

  /** Поток, а не файл: перемотки нет, длительность неизвестна. */
  get isStream(): boolean {
    const path = this.track?.path ?? '';
    return path.startsWith('http://') || path.startsWith('https://');
  }

  /** Есть ли у текущего трека текст песни (по тегам, без чтения самого текста). */
  get hasLyrics(): boolean {
    return this.tags?.hasLyrics ?? false;
  }

  /**
   * Текст берём по требованию: в тегах это целые куплеты, и тянуть их
   * при каждой смене трека незачем. Повторный вызов для того же файла
   * ничего не читает.
   */
  async loadLyrics(): Promise<void> {
    const path = this.track?.path;
    if (!path || this.lyricsPath === path) return;
    try {
      const text = await api.lyricsRead(path);
      if (this.track?.path !== path) return;
      this.lyrics = text;
      this.lyricsPath = path;
    } catch (err) {
      console.warn('[ringloft] текст песни не прочитался', err);
    }
  }

  /**
   * Поиск текста в интернете. Панель показывает текст по флагу в тегах,
   * поэтому после находки его надо взвести — иначе текст найден, а не виден.
   */
  async fetchLyrics(): Promise<void> {
    const path = this.track?.path;
    if (!path || this.lyricsSearch === 'busy') return;

    this.lyricsSearch = 'busy';
    this.lyricsMatched = null;
    try {
      const result = await api.lyricsFetch(path);
      // Трек мог успеть сменится, пока ходили в сеть.
      if (this.track?.path !== path) return;

      this.lyricsSearch = result.status;
      this.lyricsMatched = result.matched;
      this.lyricsSaved = result.saved;
      if (result.lyrics && result.lyrics.lines.length > 0) {
        this.lyrics = result.lyrics;
        this.lyricsPath = path;
        if (this.tags) this.tags = { ...this.tags, hasLyrics: true };
      }
    } catch (err) {
      console.warn('[ringloft] текст песни не нашёлся', err);
      this.lyricsSearch = 'missing';
    }
  }

  /** Теги и обложка текущего трека. Старый blob обязательно отзываем. */
  /** Теги или обложку играющего файла поменяли — перечитать. */
  async reloadMetadata(): Promise<void> {
    if (this.track) await this.loadMetadata(this.track.path);
  }

  private async loadMetadata(path: string): Promise<void> {
    this.releaseCover();
    this.tags = null;
    this.lyricsSearch = 'idle';
    this.lyricsMatched = null;
    this.lyrics = null;
    this.lyricsPath = null;
    // У радио ни тегов, ни обложки в файле нет — название придёт из эфира.
    if (this.isStream) return;
    try {
      const tags = await api.tagsRead(path);
      if (this.track?.path !== path) return;
      // У трека из CUE теги образа — это данные всего диска, название и
      // исполнителя берём из плейлиста, а обложку из файла можно.
      // Текст из тега образа — это текст всего диска, а не одного куска.
      this.tags = this.track?.isRegion
        ? { ...tags, title: null, track: null, hasLyrics: false }
        : tags;

      if (!tags.coverMime) return;
      const bytes = await api.coverRead(path);
      if (this.track?.path !== path || bytes.byteLength === 0) return;
      this.coverUrl = URL.createObjectURL(new Blob([bytes], { type: tags.coverMime }));
      const tint = await dominantColor(this.coverUrl);
      if (this.track?.path === path) this.coverTint = tint;
    } catch (err) {
      // Отсутствие тегов — не повод показывать ошибку поверх плеера.
      console.warn('[ringloft] теги не прочитались', err);
    }
  }

  private releaseCover(): void {
    if (this.coverUrl) URL.revokeObjectURL(this.coverUrl);
    this.coverUrl = null;
    this.coverTint = null;
  }

  async toggle(): Promise<void> {
    await this.run(() => api.playerToggle());
  }

  async next(): Promise<void> {
    await this.run(() => api.playerNext());
  }

  async prev(): Promise<void> {
    await this.run(() => api.playerPrev());
  }

  async stop(): Promise<void> {
    await this.run(() => api.playerStop());
    this.setPosition(0);
  }

  async setVolume(volume: number): Promise<void> {
    this.volume = volume;
    await this.run(() => api.playerSetVolume(volume));
  }

  /** Выключить или вернуть звук. Логика одна на кнопку и горячую клавишу. */
  async toggleMute(): Promise<void> {
    if (this.volume > 0) {
      this.beforeMute = this.volume;
      await this.setVolume(0);
    } else {
      await this.setVolume(this.beforeMute || 0.7);
    }
  }

  async nudgeVolume(delta: number): Promise<void> {
    await this.setVolume(Math.min(1, Math.max(0, this.volume + delta)));
  }

  /** Перемотка шагом. У радио длительности нет — там перематывать нечего. */
  async nudgePosition(deltaMs: number): Promise<void> {
    if (this.durationMs <= 0) return;
    const target = Math.min(this.durationMs, Math.max(0, this.positionMs + deltaMs));
    await this.seek(target);
  }

  async setDevice(deviceId: string | null): Promise<void> {
    await this.run(() => api.playerSetDevice(deviceId));
    this.device = this.devices.find((device) => device.id === deviceId)?.name ?? null;
  }

  private async run(action: () => Promise<unknown>): Promise<void> {
    try {
      await action();
      this.error = null;
    } catch (err) {
      this.error = toErrorPayload(err);
    }
  }
}

export const player = new PlayerStore();
