import { convertFileSrc } from '@tauri-apps/api/core';
import { SvelteMap } from 'svelte/reactivity';

import { api } from '../lib/api';

/** Сколько путей уходит одним запросом: первый показ обложки — это чтение тега. */
const BATCH = 24;
/** Строки появляются на экране пачкой — ждём, пока прокрутка их наберёт. */
const WAIT_MS = 60;

/**
 * Миниатюры обложек для строк списков на телефоне. Строка говорит, что её
 * видно (`want`), стор собирает такие просьбы в пачки и спрашивает бэкенд;
 * картинку вебвью берёт сам, по asset-протоколу.
 */
class ThumbsStore {
  /** Путь трека → адрес картинки; `null` — обложки нет, повторно не просим. */
  private urls = new SvelteMap<string, string | null>();
  private asked = new Set<string>();
  private queue: string[] = [];
  private timer: ReturnType<typeof setTimeout> | null = null;

  url(path: string): string | null {
    return this.urls.get(path) ?? null;
  }

  want(path: string): void {
    if (this.asked.has(path)) return;
    this.asked.add(path);
    this.queue.push(path);
    this.timer ??= setTimeout(() => void this.flush(), WAIT_MS);
  }

  private async flush(): Promise<void> {
    this.timer = null;
    // Последние просьбы — про то, что на экране сейчас: их и берём первыми.
    const batch = this.queue.splice(-BATCH).reverse();
    if (batch.length === 0) return;
    try {
      const found = await api.coversForTracks(batch);
      batch.forEach((path, index) => {
        const file = found[index];
        this.urls.set(path, file ? convertFileSrc(file) : null);
      });
    } catch (err) {
      console.warn('[ringloft] миниатюры не пришли', err);
      // Спросим ещё раз, когда строки снова окажутся на экране.
      for (const path of batch) this.asked.delete(path);
    }
    if (this.queue.length > 0) this.timer = setTimeout(() => void this.flush(), 0);
  }
}

export const thumbs = new ThumbsStore();
