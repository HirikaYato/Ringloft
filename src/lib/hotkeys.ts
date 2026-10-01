/**
 * Горячие клавиши одной таблицей: из неё и работает обработчик, и рисуется
 * шпаргалка. Разъезжаться им нельзя — незадокументированное сочетание
 * пользователь не найдёт, а подписанное, но не работающее, хуже молчания.
 */

export type HotkeyId =
  | 'toggle'
  | 'stop'
  | 'next'
  | 'prev'
  | 'seekForward'
  | 'seekForwardFar'
  | 'seekBack'
  | 'seekBackFar'
  | 'volumeUp'
  | 'volumeDown'
  | 'mute'
  | 'shuffle'
  | 'repeat'
  | 'fullView'
  | 'upnext'
  | 'lyrics'
  | 'equalizer'
  | 'settings'
  | 'mini'
  | 'view'
  | 'help'
  | 'fontBigger'
  | 'fontSmaller'
  | 'fontReset'
  | 'search'
  | 'openFiles'
  | 'addFolder'
  | 'newTab'
  | 'closeTab'
  | 'nextTab'
  | 'prevTab'
  | 'tabByNumber';

/** `digit` вместо клавиши значит «любая цифра 1…9». */
type Combo = {
  key: string;
  /** Та же команда с другой клавиши: `+` с цифрового блока для `=`. */
  also?: string[];
  ctrl?: boolean;
  shift?: boolean;
  alt?: boolean;
};

export type Hotkey = {
  id: HotkeyId;
  /** Как сочетание выглядит в шпаргалке. */
  keys: string;
  label: string;
  group: string;
  /** Без `combo` строка только описывает то, что ловит сам список. */
  combo?: Combo;
};

const PLAYBACK = 'Воспроизведение';
const VIEWS = 'Виды и панели';
const LIST = 'Список';

export const HOTKEYS: Hotkey[] = [
  { id: 'toggle', keys: 'Пробел', label: 'Пауза или продолжить', group: PLAYBACK, combo: { key: ' ' } },
  { id: 'stop', keys: 'Ctrl+Пробел', label: 'Остановить', group: PLAYBACK, combo: { key: ' ', ctrl: true } },
  { id: 'next', keys: 'Ctrl+Вправо', label: 'Следующий трек', group: PLAYBACK, combo: { key: 'ArrowRight', ctrl: true } },
  { id: 'prev', keys: 'Ctrl+Влево', label: 'Предыдущий трек', group: PLAYBACK, combo: { key: 'ArrowLeft', ctrl: true } },
  { id: 'seekForward', keys: 'Вправо', label: 'Вперёд на 5 секунд', group: PLAYBACK, combo: { key: 'ArrowRight' } },
  {
    id: 'seekForwardFar',
    keys: 'Shift+Вправо',
    label: 'Вперёд на 30 секунд',
    group: PLAYBACK,
    combo: { key: 'ArrowRight', shift: true },
  },
  { id: 'seekBack', keys: 'Влево', label: 'Назад на 5 секунд', group: PLAYBACK, combo: { key: 'ArrowLeft' } },
  {
    id: 'seekBackFar',
    keys: 'Shift+Влево',
    label: 'Назад на 30 секунд',
    group: PLAYBACK,
    combo: { key: 'ArrowLeft', shift: true },
  },
  {
    id: 'volumeUp',
    keys: 'Ctrl+Вверх',
    label: 'Громче',
    group: PLAYBACK,
    combo: { key: 'ArrowUp', ctrl: true },
  },
  {
    id: 'volumeDown',
    keys: 'Ctrl+Вниз',
    label: 'Тише',
    group: PLAYBACK,
    combo: { key: 'ArrowDown', ctrl: true },
  },
  { id: 'mute', keys: 'M', label: 'Выключить или включить звук', group: PLAYBACK, combo: { key: 'm' } },
  { id: 'shuffle', keys: 'S', label: 'Перемешивание', group: PLAYBACK, combo: { key: 's' } },
  { id: 'repeat', keys: 'R', label: 'Повтор: выкл → список → трек', group: PLAYBACK, combo: { key: 'r' } },

  { id: 'fullView', keys: 'F', label: 'Во весь экран (выход — Esc)', group: VIEWS, combo: { key: 'f' } },
  { id: 'upnext', keys: 'Q', label: 'Дальше по порядку', group: VIEWS, combo: { key: 'q' } },
  { id: 'lyrics', keys: 'L', label: 'Текст песни', group: VIEWS, combo: { key: 'l' } },
  { id: 'equalizer', keys: 'E', label: 'Эквалайзер', group: VIEWS, combo: { key: 'e' } },
  { id: 'settings', keys: 'Ctrl+,', label: 'Настройки', group: VIEWS, combo: { key: ',', ctrl: true } },
  { id: 'mini', keys: 'Ctrl+M', label: 'Компактный режим', group: VIEWS, combo: { key: 'm', ctrl: true } },
  { id: 'view', keys: 'Ctrl+B', label: 'Плейлист или библиотека', group: VIEWS, combo: { key: 'b', ctrl: true } },
  { id: 'help', keys: 'F1', label: 'Эта шпаргалка', group: VIEWS, combo: { key: 'F1' } },
  {
    id: 'fontBigger',
    keys: 'Ctrl+=',
    label: 'Шрифт крупнее',
    group: VIEWS,
    combo: { key: '=', also: ['+'], ctrl: true },
  },
  { id: 'fontSmaller', keys: 'Ctrl+−', label: 'Шрифт мельче', group: VIEWS, combo: { key: '-', ctrl: true } },
  { id: 'fontReset', keys: 'Ctrl+0', label: 'Обычный размер шрифта', group: VIEWS, combo: { key: '0', ctrl: true } },

  { id: 'search', keys: 'Ctrl+F', label: 'Поиск по списку', group: LIST, combo: { key: 'f', ctrl: true } },
  { id: 'openFiles', keys: 'Ctrl+O', label: 'Открыть файлы', group: LIST, combo: { key: 'o', ctrl: true } },
  {
    id: 'addFolder',
    keys: 'Ctrl+Shift+O',
    label: 'Добавить папку',
    group: LIST,
    combo: { key: 'o', ctrl: true, shift: true },
  },
  { id: 'newTab', keys: 'Ctrl+T', label: 'Новая вкладка', group: LIST, combo: { key: 't', ctrl: true } },
  { id: 'closeTab', keys: 'Ctrl+W', label: 'Закрыть вкладку', group: LIST, combo: { key: 'w', ctrl: true } },
  {
    id: 'nextTab',
    keys: 'Ctrl+Tab',
    label: 'Следующая вкладка',
    group: LIST,
    combo: { key: 'Tab', ctrl: true },
  },
  {
    id: 'prevTab',
    keys: 'Ctrl+Shift+Tab',
    label: 'Предыдущая вкладка',
    group: LIST,
    combo: { key: 'Tab', ctrl: true, shift: true },
  },
  {
    id: 'tabByNumber',
    keys: 'Alt+1…9',
    label: 'Вкладка по номеру',
    group: LIST,
    combo: { key: 'digit', alt: true },
  },
];

/** То, что ловит сам список — в шпаргалке это нужно, в обработчике нет. */
export const LIST_KEYS: { keys: string; label: string }[] = [
  { keys: 'Вверх / Вниз', label: 'Выбрать строку' },
  { keys: 'Shift+Вверх / Вниз', label: 'Расширить выделение' },
  { keys: 'Enter', label: 'Играть выбранное' },
  { keys: 'Delete', label: 'Убрать из списка' },
  { keys: 'Ctrl+A', label: 'Выделить всё' },
  { keys: 'Правый клик', label: 'Меню строки' },
];

/** Порядок групп в шпаргалке. */
export const HOTKEY_GROUPS = [PLAYBACK, VIEWS, LIST];

/** Физическая клавиша для латинской буквы или знака: `KeyM`, `Comma`… */
const CODES: Record<string, string> = {
  ',': 'Comma',
  '.': 'Period',
  '=': 'Equal',
  '-': 'Minus',
  '0': 'Digit0',
};

function codeFor(key: string): string | undefined {
  return /^[a-z]$/i.test(key) ? `Key${key.toUpperCase()}` : CODES[key];
}

/**
 * Сначала сверяем символ, а если раскладка не латинская (на русской M — это
 * «ь»), то физическую клавишу. Иначе при русской раскладке не работала
 * половина сочетаний. Латинские раскладки по коду не сверяем: на AZERTY
 * клавиша с кодом KeyQ печатает «a», и пусть она остаётся «a».
 */
function keyMatches(event: KeyboardEvent, combo: Combo): boolean {
  if (combo.key === 'digit') return /^[1-9]$/.test(event.key);
  const pressed = event.key.toLowerCase();
  if ([combo.key, ...(combo.also ?? [])].some((key) => key.toLowerCase() === pressed)) {
    return true;
  }
  const latin = /^[\x20-\x7e]$/.test(event.key);
  return !latin && event.key.length === 1 && event.code === codeFor(combo.key);
}

function matches(event: KeyboardEvent, combo: Combo): boolean {
  const key = keyMatches(event, combo);
  // Модификаторы сверяем полностью: иначе Ctrl+→ сработал бы и как «→».
  return (
    key &&
    event.ctrlKey === Boolean(combo.ctrl) &&
    event.shiftKey === Boolean(combo.shift) &&
    event.altKey === Boolean(combo.alt) &&
    !event.metaKey
  );
}

export function findHotkey(event: KeyboardEvent): Hotkey | undefined {
  return HOTKEYS.find((hotkey) => hotkey.combo && matches(event, hotkey.combo));
}
