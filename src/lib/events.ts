/** Имена событий бэкенда в одном месте + типизированная подписка. */
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import type {
  CoversFinished,
  CoversProgress,
  ErrorPayload,
  LyricsFinished,
  LyricsProgress,
  GainFinished,
  GainProgress,
  PlaybackStatus,
  PlayerTick,
  ScanProgress,
  SleepState,
  TrackInfo,
} from './api';

export type ScanFinished = { added: number; updated: number; removed: number };

export type EventMap = {
  'player:tick': PlayerTick;
  'player:track': TrackInfo;
  'player:status': PlaybackStatus;
  /** Радиостанция сменила название трека прямо в потоке. */
  'player:stream-title': string;
  'player:ended': null;
  /** Таймер сна сработал или снялся сам. */
  'player:sleep': SleepState;
  'app:error': ErrorPayload;
  'playlist:changed': null;
  'library:scan-progress': ScanProgress;
  'library:scan-finished': ScanFinished;
  /** Свой подсчёт громкости: ход работы и итог. */
  'gain:progress': GainProgress;
  'gain:finished': GainFinished;
  /** Пакетный поиск обложек. */
  'covers:progress': CoversProgress;
  'covers:finished': CoversFinished;
  /** Пакетная загрузка текстов. */
  'lyrics:progress': LyricsProgress;
  'lyrics:finished': LyricsFinished;
};

export function on<K extends keyof EventMap>(
  event: K,
  handler: (payload: EventMap[K]) => void,
): Promise<UnlistenFn> {
  return listen<EventMap[K]>(event, ({ payload }) => handler(payload));
}
