import { player } from '../stores/player.svelte.ts';
import { flung } from './swipe';

/**
 * Перелистывание трека жестом: карточка едет за пальцем, а при отпускании
 * либо улетает — и тогда трек переключается, — либо возвращается на место.
 *
 * Новая карточка въезжает с другой стороны, только когда трек сменился на
 * самом деле. Не сменился (список кончился, назад некуда) — прежняя
 * возвращается оттуда, куда улетела: так видно, что дальше ничего нет.
 */
const OUT_MS = 170;
const IN_MS = 260;
/** Сколько ждать смены трека, прежде чем признать, что её не будет. */
const SWITCH_WAIT_MS = 700;
/** Сколько ждать обложку нового трека: без неё въехала бы пустая карточка. */
const COVER_WAIT_MS = 400;

export class TrackFling {
  offset = $state(0);
  busy = $state(false);
  private duration = $state(0);
  private easing = $state('linear');

  /**
   * `size` — на сколько улетать (ширина карточки), `arrive` зовётся перед
   * въездом нового трека: там карточка забирает его данные.
   */
  constructor(
    private readonly size: () => number,
    private readonly arrive?: () => void,
  ) {}

  /** Положение карточки: сдвиг и лёгкое угасание к краю. */
  get style(): string {
    const size = Math.max(1, this.size());
    const fade = 1 - Math.min(1, Math.abs(this.offset) / size) * 0.6;
    return (
      `transform: translateX(${this.offset}px); opacity: ${fade};` +
      ` transition: transform ${this.duration}ms ${this.easing}, opacity ${this.duration}ms ${this.easing};`
    );
  }

  drag(delta: number): void {
    if (this.busy) return;
    this.slide(delta, 0);
  }

  async drop(delta: number, velocity: number): Promise<void> {
    if (this.busy) return;
    const size = this.size();
    if (!flung(delta, velocity, size)) {
      this.slide(0, IN_MS);
      return;
    }
    this.busy = true;
    const dir = delta < 0 ? -1 : 1;
    const before = player.track?.itemId;
    this.slide(dir * size, OUT_MS, 'cubic-bezier(0.4, 0, 1, 1)');
    // Переключаем сразу: пока карточка улетает, следующий трек уже открывается.
    const switching = dir < 0 ? player.next() : player.prev();
    await Promise.all([wait(OUT_MS), switching]);
    if (await until(() => player.track?.itemId !== before, SWITCH_WAIT_MS)) {
      await until(coverReady, COVER_WAIT_MS);
      this.arrive?.();
      this.slide(-dir * size, 0);
      await frames(2);
    }
    this.slide(0, IN_MS);
    await wait(IN_MS);
    this.busy = false;
  }

  private slide(offset: number, duration: number, easing = 'cubic-bezier(0.2, 0, 0, 1)'): void {
    this.duration = duration;
    this.easing = easing;
    this.offset = offset;
  }
}

/** Обложка играющего трека готова: пришла или точно известно, что её нет. */
export function coverReady(): boolean {
  if (player.isStream) return true;
  const tags = player.tags;
  return tags !== null && (!tags.coverMime || player.coverUrl !== null);
}

function wait(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function frames(count: number): Promise<void> {
  return new Promise((resolve) => {
    const step = (left: number) => (left <= 0 ? resolve() : requestAnimationFrame(() => step(left - 1)));
    step(count);
  });
}

async function until(check: () => boolean, timeoutMs: number): Promise<boolean> {
  const deadline = performance.now() + timeoutMs;
  while (!check()) {
    if (performance.now() > deadline) return false;
    await wait(30);
  }
  return true;
}
