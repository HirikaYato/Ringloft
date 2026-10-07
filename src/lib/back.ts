/**
 * Системная кнопка «назад» на телефоне. Вебвью отдаёт её истории страницы
 * (`handleBackNavigation` в MainActivity), поэтому каждый открытый слой —
 * экран «сейчас играет», лист, страница внутри библиотеки — кладёт в историю
 * запись, а «назад» закрывает верхний. Когда слоёв нет, история пуста, и
 * кнопка делает обычное: сворачивает приложение.
 *
 * Закрывать слой надо **только** через функцию, которую вернул `pushBack`:
 * она уходит тем же путём, что и кнопка, и история не расходится с экраном.
 */
interface Layer {
  depth: number;
  close: () => void;
}

const layers: Layer[] = [];
let depth = 0;
let listening = false;

function listen(): void {
  if (listening) return;
  listening = true;
  window.addEventListener('popstate', (event) => {
    // В записи истории лежит глубина, на которую вернулись: закрываем всё,
    // что выше. Обычно это один слой.
    const target = (event.state as { depth?: number } | null)?.depth ?? 0;
    while (layers.length > 0 && (layers.at(-1)?.depth ?? 0) > target) {
      layers.pop()?.close();
    }
    depth = target;
  });
}

/** Открывает слой. Возвращает функцию, которой его закрывают. */
export function pushBack(close: () => void): () => void {
  listen();
  depth += 1;
  const layer = { depth, close };
  layers.push(layer);
  history.pushState({ depth }, '');
  return () => {
    if (layers.at(-1) === layer) history.back();
    else forget(layer);
  };
}

/**
 * Слой исчез сам (компонент убрали с экрана), минуя «назад». Запись в
 * истории остаётся лишней — одно нажатие «назад» потом уйдёт впустую, зато
 * ничего чужого не закроется.
 */
function forget(layer: Layer): void {
  const index = layers.indexOf(layer);
  if (index < 0) return;
  layers.splice(index, 1);
  layer.close();
}
