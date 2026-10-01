/** Пакетная загрузка текстов: ход фоновой задачи и её запуск из меню. */
import { api, toErrorPayload, type LyricsFinished, type LyricsProgress } from '../lib/api';
import { on } from '../lib/events';

class LyricsScanStore {
  running = $state(false);
  progress = $state<LyricsProgress | null>(null);
  /** Итог последней задачи — висит, пока не запустят следующую. */
  last = $state<LyricsFinished | null>(null);

  async init(): Promise<void> {
    await Promise.all([
      on('lyrics:progress', (progress) => {
        this.running = true;
        this.progress = progress;
      }),
      on('lyrics:finished', (finished) => {
        this.running = false;
        this.progress = null;
        this.last = finished;
      }),
    ]);
    this.running = await api.lyricsScanning().catch(() => false);
  }

  /** `false` — задача уже идёт или искать нечего. */
  async scan(paths: string[]): Promise<boolean> {
    return this.start(() => api.lyricsScanStart(paths));
  }

  async scanPlaylist(): Promise<boolean> {
    return this.start(() => api.lyricsScanPlaylist());
  }

  async scanLibrary(): Promise<boolean> {
    return this.start(() => api.lyricsScanLibrary());
  }

  private async start(request: () => Promise<boolean>): Promise<boolean> {
    try {
      const started = await request();
      if (started) {
        this.running = true;
        this.last = null;
      }
      return started;
    } catch (err) {
      console.error('[ringloft] загрузка текстов', toErrorPayload(err));
      return false;
    }
  }

  cancel(): void {
    void api.lyricsScanCancel().catch(() => undefined);
  }
}

export const lyricsScan = new LyricsScanStore();
