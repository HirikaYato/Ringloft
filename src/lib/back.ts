/**
 * Системная кнопка «назад» на телефоне. Каждый открытый слой — экран
 * «сейчас играет», лист, окно, страница внутри библиотеки, режим поиска —
 * встаёт в стопку, и «назад» закрывает верхний. Когда закрывать нечего,
 * приложение сворачивается.
 *
 * О нажатии сообщает Activity (событие `app:back`). Истории вебвью для этого
 * не хватает: записи, созданные без жеста пользователя, Chrome при «назад»
 * пропускает — слой, открытый программно, так не закрылся бы.
 */
import { api } from './api';
import { on } from './events';

interface Layer {
  close: () => void;
  /** Слой сейчас на экране. У раздела, с которого ушли, слои остаются — но «назад» не про них. */
  shown: () => boolean;
}

const layers: Layer[] = [];

/**
 * Открывает слой. Возвращает функцию, которой его закрывают. `shown` нужен
 * слоям внутри раздела: раздел могут спрятать, не закрывая в нём ничего.
 */
export function pushBack(close: () => void, shown: () => boolean = () => true): () => void {
  layers.push({ close, shown });
  return () => {
    if (forgetBack(close)) close();
  };
}

/** Слой закрылся сам, без «назад»: вынимаем его из стопки, ничего не вызывая. */
export function forgetBack(close: () => void): boolean {
  const index = layers.findIndex((layer) => layer.close === close);
  if (index < 0) return false;
  layers.splice(index, 1);
  return true;
}

/** Подписка на кнопку — один раз, из корневого макета телефона. */
export async function listenBack(): Promise<void> {
  await on('app:back', () => {
    // Верхний из тех, что на экране.
    let index = layers.length - 1;
    while (index >= 0 && !layers[index]?.shown()) index -= 1;
    if (index < 0) {
      void api.appToBackground();
      return;
    }
    const [layer] = layers.splice(index, 1);
    layer?.close();
  });
}
