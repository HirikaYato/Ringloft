/** Свой подсчёт громкости: ход фоновой задачи и запуск её из меню. */
import { api, toErrorPayload, type GainFinished, type GainProgress } from '../lib/api';
import { on } from '../lib/events';

class GainStore {
  running = $state(false);
  progress = $state<GainProgress | null>(null);
  /** Итог последней задачи — показывается, пока не запустят следующую. */
  last = $state<GainFinished | null>(null);

  async init(): Promise<void> {
    await Promise.all([
      on('gain:progress', (progress) => {
        this.running = true;
        this.progress = progress;
      }),
      on('gain:finished', (finished) => {
        this.running = false;
        this.progress = null;
        this.last = finished;
      }),
    ]);
    this.running = await api.gainScanning().catch(() => false);
  }

  /** `false` — задача уже идёт или считать нечего. */
  async scan(paths: string[], force = false): Promise<boolean> {
    return this.start(() => api.gainScanStart(paths, force));
  }

  async scanPlaylist(force = false): Promise<boolean> {
    return this.start(() => api.gainScanPlaylist(force));
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
      console.error('[ringloft] подсчёт громкости', toErrorPayload(err));
      return false;
    }
  }

  cancel(): void {
    void api.gainScanCancel().catch(() => undefined);
  }
}

export const gain = new GainStore();
