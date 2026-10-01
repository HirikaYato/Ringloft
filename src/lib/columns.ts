/**
 * Колонки плейлиста — одной таблицей: из неё рисуются и заголовок, и строки
 * списка, и результаты поиска, и меню выбора колонок. Набор и ширины —
 * в настройках (`ui.playlistColumns`), порядок в меню — как здесь.
 */
import type { ColumnId, ColumnSetting, PlaylistRow, SortKey } from './api';
import { formatTime } from './format';

/** `title` — главная колонка, `dim` — второстепенный текст, `mono` — числа. */
export type ColumnKind = 'title' | 'dim' | 'mono' | 'stars';

export type ColumnDef = {
  id: ColumnId;
  label: string;
  /** Подпись в меню, если в заголовке сокращена. */
  menuLabel?: string;
  sort?: SortKey;
  /** Ширина в сетке, пока её не тянули руками. */
  track: string;
  kind: ColumnKind;
  value: (row: PlaylistRow, index: number) => string;
};

const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;
const extension = (path: string) => {
  const name = fileName(path);
  const dot = name.lastIndexOf('.');
  return dot > 0 && !path.startsWith('http') ? name.slice(dot + 1).toUpperCase() : '';
};

export const COLUMN_DEFS: ColumnDef[] = [
  { id: 'num', label: '#', menuLabel: 'Номер', sort: 'trackNo', track: 'var(--col-num)', kind: 'mono', value: (_row, index) => String(index + 1) },
  { id: 'title', label: 'Название', sort: 'title', track: 'minmax(120px, 2fr)', kind: 'title', value: (row) => row.title },
  { id: 'artist', label: 'Исполнитель', sort: 'artist', track: 'minmax(90px, 1.2fr)', kind: 'dim', value: (row) => row.artist ?? '' },
  { id: 'album', label: 'Альбом', sort: 'album', track: 'minmax(90px, 1.2fr)', kind: 'dim', value: (row) => row.album ?? '' },
  { id: 'year', label: 'Год', sort: 'year', track: 'calc(var(--fs-md) * 3.8)', kind: 'mono', value: (row) => (row.year ? String(row.year) : '') },
  { id: 'genre', label: 'Жанр', sort: 'genre', track: 'minmax(70px, 0.8fr)', kind: 'dim', value: (row) => row.genre ?? '' },
  { id: 'duration', label: 'Время', sort: 'duration', track: 'var(--col-time)', kind: 'mono', value: (row) => (row.durationMs != null ? formatTime(row.durationMs) : '') },
  { id: 'format', label: 'Формат', track: 'calc(var(--fs-md) * 5.8)', kind: 'mono', value: (row) => extension(row.path) },
  {
    id: 'bitrate',
    label: 'Кбит/с',
    menuLabel: 'Битрейт',
    sort: 'bitrate',
    track: 'calc(var(--fs-md) * 5)',
    kind: 'mono',
    value: (row) => (row.bitrateKbps ? String(row.bitrateKbps) : ''),
  },
  {
    id: 'plays',
    label: 'Слуш.',
    menuLabel: 'Прослушивания',
    track: 'calc(var(--fs-md) * 4.8)',
    kind: 'mono',
    value: (row) => (row.playCount ? String(row.playCount) : ''),
  },
  {
    id: 'rating',
    label: 'Оценка',
    track: 'calc(var(--fs-md) * 5.6)',
    kind: 'stars',
    value: (row) => (row.rating ? '★'.repeat(row.rating) + '☆'.repeat(5 - row.rating) : ''),
  },
  { id: 'file', label: 'Файл', sort: 'path', track: 'minmax(100px, 1.4fr)', kind: 'dim', value: (row) => fileName(row.path) },
];

const BY_ID = new Map(COLUMN_DEFS.map((def) => [def.id, def]));

export const DEFAULT_COLUMNS: ColumnSetting[] = (['num', 'title', 'artist', 'album', 'duration'] as ColumnId[]).map(
  (id) => ({ id, width: null }),
);

export function columnDef(id: ColumnId): ColumnDef {
  return BY_ID.get(id) ?? COLUMN_DEFS[1]!;
}

/** `grid-template-columns` для заголовка и строк. */
export function gridTemplate(columns: ColumnSetting[]): string {
  return columns.map((column) => (column.width ? `${column.width}px` : columnDef(column.id).track)).join(' ');
}

/** Показать или спрятать колонку; новая встаёт на своё место по таблице. */
export function toggleColumn(columns: ColumnSetting[], id: ColumnId): ColumnSetting[] {
  if (id === 'title') return columns;
  if (columns.some((column) => column.id === id)) return columns.filter((column) => column.id !== id);
  const rank = (columnId: ColumnId) => COLUMN_DEFS.findIndex((def) => def.id === columnId);
  const next = [...columns, { id, width: null }];
  return next.sort((a, b) => rank(a.id) - rank(b.id));
}
