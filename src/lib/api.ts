/**
 * Типизированные обёртки над invoke. Единственное место, где фронт знает
 * имена команд: дальше по коду — только `api.*`.
 */
import { invoke, type Channel } from '@tauri-apps/api/core';

import type { AppInfo } from '../bindings/AppInfo';
import type { AppPaths } from '../bindings/AppPaths';
import type { AudioDevice } from '../bindings/AudioDevice';
import type { ErrorPayload } from '../bindings/ErrorPayload';
import type { FileOpResult } from '../bindings/FileOpResult';
import type { GainFinished } from '../bindings/GainFinished';
import type { GainProgress } from '../bindings/GainProgress';
import type { AlbumRow } from '../bindings/AlbumRow';
import type { DuplicateGroup } from '../bindings/DuplicateGroup';
import type { LibraryStats } from '../bindings/LibraryStats';
import type { NamedCount } from '../bindings/NamedCount';
import type { LastfmStatus } from '../bindings/LastfmStatus';
import type { LibraryTrack } from '../bindings/LibraryTrack';
import type { SmartList } from '../bindings/SmartList';
import type { SmartRule } from '../bindings/SmartRule';
import type { SmartRules } from '../bindings/SmartRules';
import type { Lyrics } from '../bindings/Lyrics';
import type { LyricsFetch } from '../bindings/LyricsFetch';
import type { LyricsFetchStatus } from '../bindings/LyricsFetchStatus';
import type { LyricsFinished } from '../bindings/LyricsFinished';
import type { LyricsProgress } from '../bindings/LyricsProgress';
import type { RenamePreview } from '../bindings/RenamePreview';
import type { PlaybackStatus } from '../bindings/PlaybackStatus';
import type { RepeatMode } from '../bindings/RepeatMode';
import type { ReplayGainMode } from '../bindings/ReplayGainMode';
import type { PlayerState } from '../bindings/PlayerState';
import type { PlayerTick } from '../bindings/PlayerTick';
import type { PlaylistMeta } from '../bindings/PlaylistMeta';
import type { PlaylistRow } from '../bindings/PlaylistRow';
import type { PlaylistSearch } from '../bindings/PlaylistSearch';
import type { PlaylistSort } from '../bindings/PlaylistSort';
import type { PlaylistSources } from '../bindings/PlaylistSources';
import type { SleepRequest } from '../bindings/SleepRequest';
import type { RateStatus } from '../bindings/RateStatus';
import type { HeadphoneHit } from '../bindings/HeadphoneHit';
import type { TagCandidate } from '../bindings/TagCandidate';
import type { CoverFetchResult } from '../bindings/CoverFetchResult';
import type { CoversFinished } from '../bindings/CoversFinished';
import type { CoversProgress } from '../bindings/CoversProgress';
import type { ColumnId } from '../bindings/ColumnId';
import type { ColumnSetting } from '../bindings/ColumnSetting';
import type { ListeningStats } from '../bindings/ListeningStats';
import type { StatsPeriod } from '../bindings/StatsPeriod';
import type { StatsBucket } from '../bindings/StatsBucket';
import type { SleepState } from '../bindings/SleepState';
import type { SourcesRefresh } from '../bindings/SourcesRefresh';
import type { PlaylistTab } from '../bindings/PlaylistTab';
import type { ScanProgress } from '../bindings/ScanProgress';
import type { SortKey } from '../bindings/SortKey';
import type { Settings } from '../bindings/Settings';
import type { TrackInfo } from '../bindings/TrackInfo';
import type { TagEdit } from '../bindings/TagEdit';
import type { TagWriteResult } from '../bindings/TagWriteResult';
import type { TrackTags } from '../bindings/TrackTags';

export type {
  CoverFetchResult,
  CoversFinished,
  CoversProgress,
  TagCandidate,
  ColumnId,
  ColumnSetting,
  HeadphoneHit,
  ListeningStats,
  StatsBucket,
  StatsPeriod,
  RateStatus,
  SleepRequest,
  SleepState,
  PlaylistSources,
  SourcesRefresh,
  AppInfo,
  AppPaths,
  AudioDevice,
  AlbumRow,
  DuplicateGroup,
  ErrorPayload,
  FileOpResult,
  RenamePreview,
  GainFinished,
  GainProgress,
  LibraryStats,
  NamedCount,
  LastfmStatus,
  LibraryTrack,
  Lyrics,
  LyricsFetch,
  LyricsFetchStatus,
  LyricsFinished,
  LyricsProgress,
  SmartList,
  SmartRule,
  SmartRules,
  ScanProgress,
  PlaybackStatus,
  RepeatMode,
  ReplayGainMode,
  PlayerState,
  PlayerTick,
  PlaylistMeta,
  PlaylistRow,
  PlaylistSearch,
  PlaylistSort,
  PlaylistTab,
  SortKey,
  Settings,
  TagEdit,
  TagWriteResult,
  TrackInfo,
  TrackTags,
};

export type BrowseScope = 'album' | 'artist' | 'genre' | 'folder';

/** Патч настроек: любое подмножество полей на любой глубине. */
export type DeepPartial<T> = {
  [K in keyof T]?: T[K] extends readonly unknown[]
    ? T[K]
    : T[K] extends object
      ? DeepPartial<T[K]>
      : T[K];
};

export function isErrorPayload(value: unknown): value is ErrorPayload {
  return (
    typeof value === 'object' &&
    value !== null &&
    'code' in value &&
    'message' in value &&
    typeof (value as ErrorPayload).message === 'string'
  );
}

/** Приводит любой отказ invoke к ErrorPayload, чтобы UI не разбирал `unknown`. */
export function toErrorPayload(value: unknown): ErrorPayload {
  if (isErrorPayload(value)) return value;
  return {
    code: 'UNKNOWN',
    message: value instanceof Error ? value.message : String(value),
    detail: null,
  };
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (raw) {
    const payload = toErrorPayload(raw);
    console.error(`[ringloft] ${command} → ${payload.code}: ${payload.message}`, payload.detail ?? '');
    throw payload;
  }
}

export const api = {
  appInfo: () => call<AppInfo>('app_info'),
  appPaths: () => call<AppPaths>('app_paths'),
  /**
   * Просит рабочий стол показать своё меню окна. `false` — не вышло, надо
   * показать своё.
   */
  windowShowMenu: () => call<boolean>('window_show_menu'),
  /**
   * «Поверх остальных окон». На Wayland приложение не может сделать это
   * само, поэтому просьбу выполняет KWin; `false` — не вышло.
   */
  windowAlwaysOnTop: (on: boolean) => call<boolean>('window_always_on_top', { on }),

  settingsGet: () => call<Settings>('settings_get'),
  settingsPatch: (patch: DeepPartial<Settings>) => call<Settings>('settings_patch', { patch }),

  playerOpen: (path: string, autoplay = true) => call<null>('player_open', { path, autoplay }),
  playerPlay: () => call<null>('player_play'),
  playerPause: () => call<null>('player_pause'),
  playerToggle: () => call<null>('player_toggle'),
  playerStop: () => call<null>('player_stop'),
  playerSeek: (positionMs: number) => call<null>('player_seek', { positionMs: Math.round(positionMs) }),
  playerSetVolume: (volume: number) => call<null>('player_set_volume', { volume }),
  playerSetDevice: (deviceId: string | null) => call<null>('player_set_device', { deviceId }),
  playerSetReplayGain: (mode: ReplayGainMode, preampDb: number) =>
    call<null>('player_set_replay_gain', { mode, preampDb }),
  playerSetEq: (enabled: boolean, preampDb: number, bandsDb: number[]) =>
    call<null>('player_set_eq', { enabled, preampDb, bandsDb }),
  playerSetCrossfade: (crossfadeMs: number) =>
    call<null>('player_set_crossfade', { crossfadeMs }),
  spectrumSubscribe: (channel: Channel<number[]>) =>
    call<null>('spectrum_subscribe', { channel }),
  spectrumUnsubscribe: () => call<null>('spectrum_unsubscribe'),
  playerState: () => call<PlayerState>('player_state'),
  audioDevices: () => call<AudioDevice[]>('audio_devices'),

  playerNext: () => call<null>('player_next'),
  playerPrev: () => call<null>('player_prev'),
  /**
   * Повтор и перемешивание запоминает ещё и играющий плейлист, поэтому это
   * команда, а не просто патч настроек. Возвращает уже нормализованные
   * настройки.
   */
  playerSetRepeat: (repeat: RepeatMode) => call<Settings>('player_set_repeat', { repeat }),
  playerSetShuffle: (shuffle: boolean) => call<Settings>('player_set_shuffle', { shuffle }),

  playlistAdd: (paths: string[], at?: number) => call<number>('playlist_add', { paths, at }),
  playlistMeta: () => call<PlaylistMeta>('playlist_meta'),
  playlistRows: (offset: number, limit: number) =>
    call<PlaylistRow[]>('playlist_rows', { offset, limit }),
  playlistRemove: (ids: number[]) => call<null>('playlist_remove', { ids }),
  playlistClear: () => call<null>('playlist_clear'),
  playlistMove: (ids: number[], toIndex: number) =>
    call<null>('playlist_move', { ids, toIndex }),
  playlistSort: (key: SortKey, ascending: boolean) =>
    call<null>('playlist_sort', { key, ascending }),
  playlistPlayIndex: (index: number) => call<null>('playlist_play_index', { index }),
  /** Поиск по открытой вкладке: название, исполнитель, альбом, имя файла. */
  playlistSearch: (query: string, offset: number, limit: number) =>
    call<PlaylistSearch>('playlist_search', { query, offset, limit }),
  /** Что заиграет дальше — с учётом перемешивания и повтора. */
  playlistUpcoming: (limit: number) => call<PlaylistRow[]>('playlist_upcoming', { limit }),
  /** Пути выбранных строк: куски CUE бэкенд пропускает сам. */
  playlistPaths: (indexes: number[]) => call<string[]>('playlist_paths', { indexes }),

  /** M3U, M3U8 или CUE — бэкенд разберётся по расширению. */
  playlistImport: (path: string) => call<number>('playlist_import', { path }),
  playlistExportM3u: (path: string) => call<number>('playlist_export_m3u', { path }),
  playlistCreateTab: (name?: string) => call<number>('playlist_create_tab', { name }),
  playlistRenameTab: (id: number, name: string) =>
    call<null>('playlist_rename_tab', { id, name }),
  playlistCloseTab: (id: number) => call<null>('playlist_close_tab', { id }),
  playlistActivateTab: (id: number) => call<null>('playlist_activate_tab', { id }),

  playlistEnqueue: (ids: number[]) => call<null>('playlist_enqueue', { ids }),
  playlistQueueRows: () => call<PlaylistRow[]>('playlist_queue_rows'),
  playlistQueueClear: () => call<null>('playlist_queue_clear'),

  lastfmStatus: () => call<LastfmStatus>('lastfm_status'),
  /** Открывает страницу подтверждения в браузере и возвращает её адрес. */
  lastfmBeginAuth: () => call<string>('lastfm_begin_auth'),
  lastfmFinishAuth: () => call<LastfmStatus>('lastfm_finish_auth'),
  lastfmDisconnect: () => call<LastfmStatus>('lastfm_disconnect'),

  libraryScanStart: () => call<boolean>('library_scan_start'),
  libraryScanCancel: () => call<null>('library_scan_cancel'),
  libraryScanning: () => call<boolean>('library_scanning'),
  libraryStats: () => call<LibraryStats>('library_stats'),
  /** Проверка коллекции: похожие треки и пропавшие файлы. */
  libraryDuplicates: () => call<DuplicateGroup[]>('library_duplicates'),
  libraryMissing: () => call<LibraryTrack[]>('library_missing'),
  /** Убирает строки из библиотеки. Файлы не трогает. */
  libraryForget: (paths: string[]) => call<number>('library_forget', { paths }),
  librarySearch: (query: string, limit = 100) =>
    call<LibraryTrack[]>('library_search', { query, limit }),
  libraryArtists: () => call<NamedCount[]>('library_artists'),
  libraryGenres: () => call<NamedCount[]>('library_genres'),
  libraryFolders: () => call<NamedCount[]>('library_folders'),
  libraryAlbums: (artist?: string) => call<AlbumRow[]>('library_albums', { artist }),
  libraryTracks: (scope: BrowseScope, value: string) =>
    call<LibraryTrack[]>('library_tracks', { scope, value }),
  libraryAlbumCover: (albumKey: string) =>
    call<string | null>('library_album_cover', { albumKey }),
  /** Треки умного списка: условия разбирает бэкенд, в SQL они не попадают. */
  librarySmartTracks: (rules: SmartRules) => call<LibraryTrack[]>('library_smart_tracks', { rules }),
  /** Оценка трека: 0 — снять, иначе 1..5. */
  librarySetRating: (path: string, rating: number) =>
    call<boolean>('library_set_rating', { path, rating }),

  libraryFoldersAdd: (paths: string[]) => call<Settings>('library_folders_add', { paths }),
  libraryFolderRemove: (path: string) => call<Settings>('library_folder_remove', { path }),

  tagsRead: (path: string) => call<TrackTags>('tags_read', { path }),
  /**
   * Запись тегов. В `edit` кладём только те поля, которые правим: остальные
   * остаются как были. Пустая строка и ноль означают «стереть поле».
   */
  tagsWrite: (paths: string[], edit: TagEdit) =>
    call<TagWriteResult>('tags_write', { paths, edit }),
  /**
   * Текст песни: сначала `.lrc` рядом с файлом, потом тег. Приходит уже
   * разобранным — с метками времени, если они есть.
   */
  lyricsRead: (path: string) => call<Lyrics>('lyrics_read', { path }),
  /**
   * Поиск текста в интернете (lrclib.net) с сохранением рядом с треком.
   * В сеть плеер ходит только отсюда — по кнопке, а не сам.
   */
  lyricsFetch: (path: string) => call<LyricsFetch>('lyrics_fetch', { path }),
  /**
   * Пакетная загрузка текстов. `false` в ответ — задача уже идёт или искать
   * нечего. Файлы с готовым текстом до сети не доходят.
   */
  lyricsScanStart: (paths: string[]) => call<boolean>('lyrics_scan_start', { paths }),
  lyricsScanPlaylist: () => call<boolean>('lyrics_scan_playlist'),
  lyricsScanLibrary: () => call<boolean>('lyrics_scan_library'),
  lyricsScanCancel: () => call<null>('lyrics_scan_cancel'),
  lyricsScanning: () => call<boolean>('lyrics_scanning'),

  /**
   * Показать файл в файловом менеджере. Команда самого плагина opener:
   * своей обёртки в Rust у неё нет, поэтому имя плагина здесь, а не в коде.
   */
  revealInFolder: (path: string) => call<null>('plugin:opener|reveal_item_in_dir', { paths: [path] }),
  /**
   * Пробное подключение к Discord: запущен ли он и принял ли Application ID.
   * Активность не ставит.
   */
  discordProbe: () => call<null>('discord_probe'),

  /** Обложки из интернета: альбом, играющий трек, все альбомы без обложки. */
  coversFetchAlbum: (albumKey: string) => call<CoverFetchResult>('covers_fetch_album', { albumKey }),
  coversFetchTrack: (path: string) => call<CoverFetchResult>('covers_fetch_track', { path }),
  coversScanStart: () => call<boolean>('covers_scan_start'),
  coversScanCancel: () => call<null>('covers_scan_cancel'),
  coversScanning: () => call<boolean>('covers_scanning'),

  /** Перетащили вкладку: встаёт перед `before`, без него — в конец. */
  playlistMoveTab: (id: number, before: number | null) => call<null>('playlist_move_tab', { id, before }),

  /** Теги по звуку: варианты для одного файла и жанр альбома из MusicBrainz. */
  identifyTrack: (path: string) => call<TagCandidate[]>('identify_track', { path }),
  identifyGenre: (releaseGroupId: string) => call<string | null>('identify_genre', { releaseGroupId }),

  /** Итоги прослушивания за период. */
  libraryListening: (period: StatsPeriod) => call<ListeningStats>('library_listening', { period }),

  /** Профиль наушников из базы AutoEQ или из своего файла. */
  headphonesSearch: (query: string) => call<HeadphoneHit[]>('headphones_search', { query }),
  headphonesApply: (hit: HeadphoneHit) => call<Settings>('headphones_apply', { hit }),
  headphonesImport: (path: string) => call<Settings>('headphones_import', { path }),
  headphonesSetEnabled: (enabled: boolean) => call<Settings>('headphones_set_enabled', { enabled }),

  /** Частота без пересчёта: состояние, режим и разрешение PipeWire. */
  audioRateStatus: () => call<RateStatus>('audio_rate_status'),
  playerSetExactRate: (exact: boolean) => call<null>('player_set_exact_rate', { exact }),
  /** Меняет настройку всей системы — только по кнопке пользователя. */
  pipewireAllowRates: (enabled: boolean) => call<RateStatus>('pipewire_allow_rates', { enabled }),

  /** Таймер сна и «остановить после этого трека». */
  sleepSet: (request: SleepRequest) => call<SleepState>('sleep_set', { request }),
  sleepState: () => call<SleepState>('sleep_state'),

  /** Установленные шрифты с кириллицей; на Windows список пустой. */
  fontsInstalled: () => call<string[]>('fonts_installed'),

  /**
   * Убрать файлы в корзину системы. Мимо корзины плеер не удаляет ничего:
   * оттуда файл можно вернуть.
   */
  filesToTrash: (paths: string[]) => call<FileOpResult>('files_to_trash', { paths }),
  /** Что получится из шаблона — до того, как что-то произойдёт. */
  filesRenamePreview: (paths: string[], pattern: string) =>
    call<RenamePreview[]>('files_rename_preview', { paths, pattern }),
  filesRename: (paths: string[], pattern: string) =>
    call<FileOpResult>('files_rename', { paths, pattern }),

  /**
   * Перенос выбранных строк в другую вкладку (перетаскиванием на ярлык).
   * `copy` — оставить их и в исходной. В ответ — сколько строк уехало.
   */
  playlistTransfer: (indexes: number[], tabId: number, copy = false) =>
    call<number>('playlist_transfer', { indexes, tabId, copy }),
  /** Убирает из списка строки, файлов которых больше нет. */
  playlistDropMissing: () => call<number>('playlist_drop_missing'),

  /** Папки-источники списка: по ним он обновляется. */
  playlistSources: (id: number) => call<PlaylistSources>('playlist_sources', { id }),
  playlistSetSources: (id: number, sources: string[]) =>
    call<null>('playlist_set_sources', { id, sources }),
  /** Вернуть убранные руками файлы: следующее обновление их допишет. */
  playlistRestoreExcluded: (id: number) => call<null>('playlist_restore_excluded', { id }),
  /** Обновить по папкам; без `id` — все списки с источниками. */
  playlistRefreshSources: (id?: number) =>
    call<SourcesRefresh>('playlist_refresh_sources', { id: id ?? null }),

  /**
   * Свой подсчёт громкости. `false` в ответ — задача уже идёт или считать
   * нечего. `force` пересчитывает и те файлы, где метка уже есть.
   */
  gainScanStart: (paths: string[], force = false) =>
    call<boolean>('gain_scan_start', { paths, force }),
  /** То же для всей открытой вкладки: пути собирает Rust. */
  gainScanPlaylist: (force = false) => call<boolean>('gain_scan_playlist', { force }),
  gainScanCancel: () => call<null>('gain_scan_cancel'),
  gainScanning: () => call<boolean>('gain_scanning'),

  /** Обложка приходит сырыми байтами, поэтому ArrayBuffer, а не base64. */
  coverRead: (path: string) => call<ArrayBuffer>('cover_read', { path }),
};
