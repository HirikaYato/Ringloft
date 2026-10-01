/** Обложки из интернета: одиночные запросы и пакетная задача по библиотеке. */
import { api, toErrorPayload, type CoverFetchResult, type CoversFinished, type CoversProgress } from '../lib/api';
import { on } from '../lib/events';
import { library } from './library.svelte.ts';
import { notice } from './notice.svelte.ts';
import { player } from './player.svelte.ts';

class CoversStore {
  running = $state(false);
  progress = $state<CoversProgress | null>(null);
  last = $state<CoversFinished | null>(null);
  /** Какой альбом ищем по просьбе — чтобы кнопка показала «ищу…». */
  busyAlbum = $state<string | null>(null);

  async init(): Promise<void> {
    await Promise.all([
      on('covers:progress', (progress) => {
        this.running = true;
        this.progress = progress;
      }),
      on('covers:finished', (finished) => {
        this.running = false;
        this.progress = null;
        this.last = finished;
        void library.loadMode(library.mode);
      }),
    ]);
    this.running = await api.coversScanning().catch(() => false);
  }

  async fetchAlbum(albumKey: string): Promise<void> {
    this.busyAlbum = albumKey;
    try {
      const result = await api.coversFetchAlbum(albumKey);
      report(result);
      if (result.found) await library.refreshCover(albumKey);
    } catch (err) {
      notice.show(`Обложка не нашлась: ${toErrorPayload(err).message}`);
    } finally {
      this.busyAlbum = null;
    }
  }

  async fetchTrack(path: string): Promise<void> {
    notice.show('Ищу обложку…', 20_000);
    try {
      const result = await api.coversFetchTrack(path);
      report(result);
      if (result.found) await player.reloadMetadata();
    } catch (err) {
      notice.show(`Обложка не нашлась: ${toErrorPayload(err).message}`);
    }
  }

  async scanMissing(): Promise<void> {
    try {
      if (await api.coversScanStart()) {
        this.running = true;
        this.last = null;
      }
    } catch (err) {
      console.error('[ringloft] поиск обложек', toErrorPayload(err));
    }
  }

  cancel(): void {
    void api.coversScanCancel().catch(() => undefined);
  }
}

function report(result: CoverFetchResult) {
  if (!result.found) {
    notice.show('Обложку не нашёл ни в iTunes, ни в Cover Art Archive');
  } else if (result.failed > 0) {
    notice.show(`Обложка из ${result.source}: вписана в ${result.written}, не удалось ${result.failed}`);
  } else {
    notice.show(`Обложка из ${result.source} вписана в ${result.written} ${result.written === 1 ? 'файл' : 'файлов'}`);
  }
}

export const covers = new CoversStore();
