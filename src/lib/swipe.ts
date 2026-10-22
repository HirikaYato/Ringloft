/**
 * Жест «потянуть пальцем»: элемент едет за пальцем по одной оси, решение
 * принимается при отпускании — по расстоянию или по скорости броска.
 *
 * Ось выбирается один раз, когда палец ушёл дальше `SLOP`, и дальше жест
 * держится её: диагональ не дёргает картинку в обе стороны. Если ось чужая,
 * жест отпускается сразу — его подберёт вложенный или внешний обработчик
 * (так обложка ловит горизонталь, а плеер вокруг неё — вертикаль).
 *
 * Прокрутку вебвью по своей оси элемент должен запретить сам
 * (`touch-action`), иначе вместо `pointermove` придёт `pointercancel`.
 */
export type Axis = 'x' | 'y';

export interface SwipeHandlers {
  /** Какие оси ловить прямо сейчас (например, пока нет анимации). */
  axes: () => { x?: boolean; y?: boolean };
  move: (axis: Axis, delta: number) => void;
  /** `velocity` — пиксели в миллисекунду по оси жеста, со знаком. */
  end: (axis: Axis, delta: number, velocity: number) => void;
}

const SLOP = 10;
/** Скорость считаем по последним миллисекундам, а не по всему жесту. */
const WINDOW_MS = 90;

export function swipe(handlers: SwipeHandlers) {
  return (node: HTMLElement) => {
    let start: { x: number; y: number; id: number } | null = null;
    let axis: Axis | null = null;
    let samples: { t: number; v: number }[] = [];
    let swallowClick = false;

    const delta = (event: PointerEvent) =>
      axis === 'x' ? event.clientX - (start?.x ?? 0) : event.clientY - (start?.y ?? 0);

    function down(event: PointerEvent) {
      if (!event.isPrimary || event.button !== 0) return;
      start = { x: event.clientX, y: event.clientY, id: event.pointerId };
      axis = null;
      samples = [];
    }

    function move(event: PointerEvent) {
      if (!start || event.pointerId !== start.id) return;
      if (!axis) {
        const dx = event.clientX - start.x;
        const dy = event.clientY - start.y;
        if (Math.hypot(dx, dy) < SLOP) return;
        const wanted: Axis = Math.abs(dx) > Math.abs(dy) ? 'x' : 'y';
        if (!handlers.axes()[wanted]) {
          start = null;
          return;
        }
        axis = wanted;
        node.setPointerCapture(event.pointerId);
      }
      const value = delta(event);
      samples.push({ t: event.timeStamp, v: value });
      while (samples.length > 2 && event.timeStamp - (samples[0]?.t ?? 0) > WINDOW_MS) samples.shift();
      handlers.move(axis, value);
    }

    function up(event: PointerEvent) {
      if (!start || event.pointerId !== start.id) return;
      const was = axis;
      const value = delta(event);
      start = null;
      axis = null;
      if (!was) return;
      // Мышью после жеста приходит ещё и click — палец его не даёт, но
      // кнопка под мышью сработала бы.
      swallowClick = true;
      setTimeout(() => (swallowClick = false), 0);
      const first = samples[0];
      const dt = first ? event.timeStamp - first.t : 0;
      handlers.end(was, value, first && dt > 0 ? (value - first.v) / dt : 0);
    }

    function cancel(event: PointerEvent) {
      if (!start || event.pointerId !== start.id) return;
      const was = axis;
      start = null;
      axis = null;
      if (was) handlers.end(was, 0, 0);
    }

    function click(event: MouseEvent) {
      if (!swallowClick) return;
      event.stopPropagation();
      event.preventDefault();
    }

    node.addEventListener('pointerdown', down);
    node.addEventListener('pointermove', move);
    node.addEventListener('pointerup', up);
    node.addEventListener('pointercancel', cancel);
    node.addEventListener('click', click, true);
    return () => {
      node.removeEventListener('pointerdown', down);
      node.removeEventListener('pointermove', move);
      node.removeEventListener('pointerup', up);
      node.removeEventListener('pointercancel', cancel);
      node.removeEventListener('click', click, true);
    };
  };
}

/** Бросок засчитан: протянули дальше доли размера или резко махнули. */
export function flung(delta: number, velocity: number, size: number, share = 0.3): boolean {
  if (Math.abs(delta) > size * share) return true;
  return Math.abs(delta) > 30 && Math.abs(velocity) > 0.45 && Math.sign(velocity) === Math.sign(delta);
}
