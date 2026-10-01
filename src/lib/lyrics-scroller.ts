/**
 * Ведёт синхронный текст песни: активная строка едет к середине колонки.
 *
 * Текст двигается `transform`-ом обёртки, а не прокруткой. Плавная прокрутка
 * в WebKitGTK укладывается в 3–6 кадров (замерено: 30–80 мс), то есть по сути
 * это прыжок, и рядом с переходом подсветки (~0,5 с) он читался как
 * оборванная анимация. Переход `transform` длится столько, сколько задано в
 * CSS, идёт на композиторе и при новой цели продолжается с текущего места.
 *
 * Колесо крутит текст само (родной прокрутки у обёртки нет), а после ручной
 * прокрутки следование молчит четыре секунды — пока текст читают, дёргать
 * его нельзя.
 */

const PAUSE_MS = 4_000;

/** Как обёртка едет к новому месту — значение `data-motion`, его ловит CSS. */
type Motion = 'follow' | 'manual' | 'none';

export class LyricsScroller {
  private offset = 0;
  private touchedAt = 0;
  /** Ведём ли текст сейчас; несинхронный листается обычной прокруткой. */
  private leading = false;

  constructor(
    private readonly box: HTMLElement,
    private readonly track: HTMLElement,
  ) {
    box.addEventListener('wheel', this.wheel, { passive: false });
    // Фокус на строке (Tab) прокрутил бы контейнер сам, хоть у него и
    // `overflow: hidden`, — и сдвиг разъехался бы с расчётом.
    box.addEventListener('scroll', this.unscroll);
  }

  destroy(): void {
    this.box.removeEventListener('wheel', this.wheel);
    this.box.removeEventListener('scroll', this.unscroll);
  }

  /** Руками тронули текст (выделение, касание) — не дёргать его. */
  touch(): void {
    this.touchedAt = performance.now();
  }

  /** Щёлкнули по строке — значит, хотят к ней, а не читать дальше. */
  release(): void {
    this.touchedAt = 0;
  }

  /**
   * Довести строку до середины. `jump` — сразу, без анимации: новый текст не
   * должен проезжать перед глазами целиком. `null` — к началу текста.
   */
  follow(target: HTMLElement | null, jump: boolean): void {
    if (!jump && performance.now() - this.touchedAt < PAUSE_MS) return;
    this.leading = true;
    const middle = this.box.clientHeight / 2;
    const center = target ? target.offsetTop + target.offsetHeight / 2 : middle;
    this.move(middle - center, jump ? 'none' : 'follow');
  }

  /** Несинхронный текст листается обычной прокруткой — сдвиг снимаем. */
  reset(): void {
    this.leading = false;
    this.offset = 0;
    this.track.style.transform = '';
  }

  private move(offset: number, motion: Motion): void {
    const lowest = Math.min(0, this.box.clientHeight - this.track.offsetHeight);
    this.offset = Math.min(0, Math.max(lowest, offset));
    this.track.dataset.motion = motion;
    this.track.style.transform = `translate3d(0, ${this.offset}px, 0)`;
    if (motion === 'none') {
      // Фиксируем положение без перехода и возвращаем переход: без чтения
      // раскладки браузер слил бы обе записи в одну и анимировал прыжок.
      void this.track.offsetHeight;
      this.track.dataset.motion = 'follow';
    }
  }

  private readonly wheel = (event: WheelEvent): void => {
    if (!this.leading) return;
    event.preventDefault();
    this.touch();
    const step = event.deltaMode === WheelEvent.DOM_DELTA_LINE ? 40 : 1;
    this.move(this.offset - event.deltaY * step, 'manual');
  };

  private readonly unscroll = (): void => {
    if (this.leading && this.box.scrollTop !== 0) this.box.scrollTop = 0;
  };
}
