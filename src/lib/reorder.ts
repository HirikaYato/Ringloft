/**
 * Перестановка вкладок перетаскиванием — для ленты (по горизонтали) и для
 * колонки плейлистов (по вертикали). Указателем, а не HTML5 drag: в Tauri на
 * Windows тот перехватывает приём файлов окна, а строки плейлиста и так
 * таскаются указателем.
 *
 * Элементы — `[data-tab-id]`; `data-fixed` — не таскается и не принимает
 * (временная вкладка всегда в конце). Клик без сдвига остаётся кликом.
 */
export type ReorderState = {
  /** Что тащат; `null` — ничего. */
  dragging: number | null;
  /** Где встанет линия-указатель, в координатах контейнера. */
  indicator: number | null;
};

export type ReorderOptions = {
  axis: 'x' | 'y';
  /** Встать перед `before`; `null` — в конец. */
  onmove: (id: number, before: number | null) => void;
  onstate: (state: ReorderState) => void;
};

const THRESHOLD = 6;
const EDGE = 28;

export function reorder(node: HTMLElement, initial: ReorderOptions) {
  let options = initial;
  let pressed: { id: number; x: number; y: number; element: HTMLElement } | null = null;
  let dragging = false;
  let before: number | null = null;

  const items = () =>
    [...node.querySelectorAll<HTMLElement>('[data-tab-id]:not([data-fixed])')].filter(
      (element) => element !== pressed?.element,
    );

  function locate(event: PointerEvent) {
    const horizontal = options.axis === 'x';
    const pointer = horizontal ? event.clientX : event.clientY;
    const box = node.getBoundingClientRect();
    const scroll = horizontal ? node.scrollLeft : node.scrollTop;
    const origin = horizontal ? box.left : box.top;
    const list = items();
    const target = list.find((element) => {
      const rect = element.getBoundingClientRect();
      return pointer < (horizontal ? rect.left + rect.width / 2 : rect.top + rect.height / 2);
    });
    before = target ? Number(target.dataset.tabId) : null;
    const anchor = target ?? list.at(-1);
    if (!anchor) return options.onstate({ dragging: pressed?.id ?? null, indicator: null });
    const rect = anchor.getBoundingClientRect();
    const edge = target ? (horizontal ? rect.left : rect.top) : horizontal ? rect.right : rect.bottom;
    options.onstate({ dragging: pressed?.id ?? null, indicator: edge - origin + scroll });

    // У края ленты прокручиваем: иначе дальние вкладки не достать.
    if (horizontal) {
      if (event.clientX < box.left + EDGE) node.scrollLeft -= 12;
      else if (event.clientX > box.right - EDGE) node.scrollLeft += 12;
    } else if (event.clientY < box.top + EDGE) node.scrollTop -= 8;
    else if (event.clientY > box.bottom - EDGE) node.scrollTop += 8;
  }

  function down(event: PointerEvent) {
    if (event.button !== 0) return;
    const element = (event.target as HTMLElement).closest<HTMLElement>('[data-tab-id]:not([data-fixed])');
    if (!element || !node.contains(element) || (event.target as HTMLElement).closest('input')) return;
    pressed = { id: Number(element.dataset.tabId), x: event.clientX, y: event.clientY, element };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up, { once: true });
  }

  function move(event: PointerEvent) {
    if (!pressed) return;
    if (!dragging) {
      if (Math.hypot(event.clientX - pressed.x, event.clientY - pressed.y) < THRESHOLD) return;
      dragging = true;
    }
    locate(event);
  }

  function up() {
    window.removeEventListener('pointermove', move);
    if (pressed && dragging) {
      const id = pressed.id;
      // Отпустили на том же месте — ничего не делаем.
      const next = pressed.element.nextElementSibling as HTMLElement | null;
      const unchanged = before === id || (next?.dataset.tabId !== undefined && Number(next.dataset.tabId) === before);
      if (!unchanged) options.onmove(id, before);
      // Клик после перетаскивания — не переключение вкладки.
      window.addEventListener('click', swallow, { capture: true, once: true });
      setTimeout(() => window.removeEventListener('click', swallow, { capture: true }), 0);
    }
    pressed = null;
    dragging = false;
    options.onstate({ dragging: null, indicator: null });
  }

  function swallow(event: MouseEvent) {
    event.stopPropagation();
    event.preventDefault();
  }

  node.addEventListener('pointerdown', down);
  return {
    update(next: ReorderOptions) {
      options = next;
    },
    destroy() {
      node.removeEventListener('pointerdown', down);
      window.removeEventListener('pointermove', move);
    },
  };
}
