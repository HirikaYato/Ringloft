/**
 * Таймер сна во фронте — только отображение. Решает бэкенд (`sleep.rs`):
 * таймеры вебвью в свёрнутом окне притормаживают, а звук должен погаснуть
 * вовремя. Обратный отсчёт считаем от момента ответа.
 */
import { api, toErrorPayload, type SleepRequest, type SleepState } from '../lib/api';
import { on } from '../lib/events';

class SleepStore {
  kind = $state<SleepState['kind']>('off');
  /** Когда сработает (по `performance.now()`); только у таймера по времени. */
  private endsAt = 0;
  /** Сколько осталось, мс; обновляется раз в секунду, пока таймер идёт. */
  remainingMs = $state(0);
  private ticker: ReturnType<typeof setInterval> | undefined;

  async init(): Promise<void> {
    await on('player:sleep', (state) => this.adopt(state));
    try {
      this.adopt(await api.sleepState());
    } catch (err) {
      console.error('[ringloft] таймер сна', toErrorPayload(err));
    }
  }

  async set(request: SleepRequest): Promise<void> {
    try {
      this.adopt(await api.sleepSet(request));
    } catch (err) {
      console.error('[ringloft] таймер сна', toErrorPayload(err));
    }
  }

  private adopt(state: SleepState): void {
    this.kind = state.kind;
    clearInterval(this.ticker);
    if (state.kind !== 'timer' || state.remainingMs === null) return;
    this.endsAt = performance.now() + state.remainingMs;
    const update = () => {
      this.remainingMs = Math.max(0, this.endsAt - performance.now());
    };
    update();
    this.ticker = setInterval(update, 1_000);
  }
}

export const sleep = new SleepStore();
