<script lang="ts">
  /**
   * Заголовок колонок: клик — сортировка, правый клик — какие колонки
   * показывать, край ячейки тянется мышью. Пока тянут, ширина живёт здесь
   * (`onchange` без сохранения), отпустили — уезжает в настройки.
   */
  import ContextMenu, { type MenuItem } from '../Common/ContextMenu.svelte';
  import { COLUMN_DEFS, DEFAULT_COLUMNS, columnDef, toggleColumn } from '../../lib/columns';
  import type { ColumnSetting, PlaylistSort, SortKey } from '../../lib/api';

  let {
    columns,
    sort,
    onsort,
    onchange,
  }: {
    columns: ColumnSetting[];
    sort: PlaylistSort;
    onsort: (key: SortKey) => void;
    /** `save` — пора в настройки; без него — только перерисовать. */
    onchange: (columns: ColumnSetting[], save: boolean) => void;
  } = $props();

  let menu = $state<{ x: number; y: number } | null>(null);
  let drag: { index: number; startX: number; startWidth: number } | null = null;
  let moved = false;

  const items = $derived<MenuItem[]>([
    ...COLUMN_DEFS.filter((def) => def.id !== 'title').map((def) => ({
      label: `${columns.some((column) => column.id === def.id) ? '✓' : ' '} ${def.menuLabel ?? def.label}`,
      action: () => onchange(toggleColumn(columns, def.id), true),
    })),
    { separator: true },
    {
      label: 'Сбросить ширины',
      disabled: columns.every((column) => !column.width),
      action: () => onchange(columns.map((column) => ({ ...column, width: null })), true),
    },
    { label: 'Колонки по умолчанию', action: () => onchange(DEFAULT_COLUMNS, true) },
  ]);

  function startResize(event: PointerEvent, index: number) {
    event.preventDefault();
    event.stopPropagation();
    const cell = (event.currentTarget as HTMLElement).parentElement;
    if (!cell) return;
    drag = { index, startX: event.clientX, startWidth: cell.getBoundingClientRect().width };
    moved = false;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function resize(event: PointerEvent) {
    if (!drag) return;
    moved = true;
    const width = Math.max(36, Math.round(drag.startWidth + event.clientX - drag.startX));
    const index = drag.index;
    onchange(
      columns.map((column, nth) => (nth === index ? { ...column, width } : column)),
      false,
    );
  }

  function endResize() {
    if (drag && moved) onchange(columns, true);
    drag = null;
  }
</script>

<div
  class="head"
  role="row"
  tabindex="-1"
  oncontextmenu={(event) => {
    event.preventDefault();
    menu = { x: event.clientX, y: event.clientY };
  }}
>
  {#each columns as column, index (column.id)}
    {@const def = columnDef(column.id)}
    <div class="cell kind-{def.kind}" class:sorted={def.sort && sort.key === def.sort}>
      <button
        class="label"
        disabled={!def.sort}
        title={def.sort ? 'Сортировать — правый клик: какие колонки показывать' : def.menuLabel ?? def.label}
        onclick={() => def.sort && onsort(def.sort)}
      >
        {def.label}
        {#if def.sort && sort.key === def.sort}
          <span class="caret">{sort.ascending ? '▲' : '▼'}</span>
        {/if}
      </button>
      {#if index < columns.length - 1}
        <span
          class="grip"
          role="separator"
          aria-orientation="vertical"
          title="Потянуть — ширина колонки"
          onpointerdown={(event) => startResize(event, index)}
          onpointermove={resize}
          onpointerup={endResize}
          onpointercancel={endResize}
        ></span>
      {/if}
    </div>
  {/each}
</div>

{#if menu}
  <ContextMenu {items} x={menu.x} y={menu.y} onclose={() => (menu = null)} />
{/if}

<style>
  .head {
    display: grid;
    grid-template-columns: var(--columns);
    border-bottom: 1px solid var(--border);
    background: var(--bg-inset);
    flex: none;
  }
  .cell {
    position: relative;
    min-width: 0;
    border-right: 1px solid var(--border);
  }
  .label {
    display: block;
    width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 5px 8px;
    font: inherit;
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    text-align: left;
    color: var(--fg-faint);
    background: none;
    border: 0;
    cursor: pointer;
  }
  .label:disabled {
    cursor: default;
  }
  .kind-mono .label,
  .kind-stars .label {
    text-align: right;
  }
  .label:hover:not(:disabled) {
    color: var(--fg);
  }
  .sorted .label {
    color: var(--accent);
  }
  .caret {
    font-size: 9px;
    margin-left: 3px;
  }
  /* Ручка для ширины — узкая полоса по правому краю ячейки. */
  .grip {
    position: absolute;
    top: 0;
    right: -4px;
    bottom: 0;
    width: 8px;
    z-index: 1;
    cursor: col-resize;
    touch-action: none;
  }
  .grip:hover {
    background: color-mix(in srgb, var(--accent) 35%, transparent);
  }
</style>
