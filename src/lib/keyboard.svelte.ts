/**
 * Экранная клавиатура на телефоне. Окно идёт под системные панели
 * (edge-to-edge), и с Android 15 система под клавиатуру его не сжимает:
 * вебвью сохраняет прежнюю высоту, а клавиатура просто ложится поверх низа.
 * Видно её только по `visualViewport` — он и становится короче.
 *
 * Высота уходит в CSS-переменную `--keyboard`: от неё сжимается корневой
 * макет и встают модальные окна, а нижние панели на это время прячутся.
 */
import { api } from './api';

export const keyboard = $state({ inset: 0 });

/** Меньше этого — не клавиатура, а дрожание вьюпорта (строка адреса и т. п.). */
const MIN_INSET = 80;

export function watchKeyboard(): void {
  const viewport = window.visualViewport;
  if (!viewport) return;
  const update = () => {
    const inset = window.innerHeight - viewport.height - viewport.offsetTop;
    keyboard.inset = inset > MIN_INSET ? Math.round(inset) : 0;
    document.documentElement.style.setProperty('--keyboard', `${keyboard.inset}px`);
    // Вебвью любит подтянуть поле к глазам прокруткой всей страницы — у нас
    // она не прокручивается, и сдвиг оставил бы пустую полосу сверху.
    if (keyboard.inset > 0 && window.scrollY !== 0) window.scrollTo(0, 0);
  };
  viewport.addEventListener('resize', update);
  viewport.addEventListener('scroll', update);
  update();
  document.addEventListener('focusin', askForKeyboard);
}

/** Поля, которым нужна клавиатура: не флажки, не ползунки. */
const TYPING = new Set(['text', 'search', 'url', 'email', 'password', 'number', 'tel']);

/**
 * Вебвью открывает клавиатуру, только если поле получило фокус прямо из
 * касания. Поиск и диалог ввода ставят фокус чуть позже — в эффекте после
 * отрисовки, — и клавиатура не выезжала. Тогда просим её у системы сами.
 */
function askForKeyboard(event: FocusEvent): void {
  const field = event.target;
  const typing =
    field instanceof HTMLTextAreaElement || (field instanceof HTMLInputElement && TYPING.has(field.type));
  if (!typing) return;
  // Ждём, не открыла ли её сама система: касание по полю так и работает.
  setTimeout(() => {
    if (keyboard.inset > 0 || document.activeElement !== field) return;
    void api.keyboardShow().catch((err) => console.warn('[ringloft] клавиатура не открылась', err));
  }, 250);
}

/** «Поиск» на клавиатуре убирает её: дальше найденное листают, а не печатают. */
export function hideKeyboardOnEnter(event: KeyboardEvent): void {
  if (event.key === 'Enter' && event.currentTarget instanceof HTMLElement) event.currentTarget.blur();
}
