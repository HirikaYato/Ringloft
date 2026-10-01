<script lang="ts">
  import type { ColumnSetting, PlaylistRow } from '../../lib/api';
  import { DEFAULT_COLUMNS, columnDef, gridTemplate } from '../../lib/columns';

  /**
   * Простой список строк плейлиста без виртуализации и выделения: им
   * показываются результаты поиска и порядок воспроизведения. Обе выборки
   * ограничены сверху, так что рисовать их целиком не страшно.
   */
  let {
    rows,
    columns = DEFAULT_COLUMNS,
    ordinal = 'index',
    empty = 'Пусто',
    onactivate,
    oncontext,
  }: {
    rows: PlaylistRow[];
    /** Те же колонки, что у плейлиста; по умолчанию — базовый набор. */
    columns?: ColumnSetting[];
    /** `index` — номер строки в списке, `position` — место в очереди. */
    ordinal?: 'index' | 'position';
    empty?: string;
    onactivate: (row: PlaylistRow) => void;
    oncontext?: (event: MouseEvent, row: PlaylistRow) => void;
  } = $props();

  const defs = $derived(columns.map((column) => columnDef(column.id)));
</script>

{#if rows.length === 0}
  <p class="empty">{empty}</p>
{:else}
  <div class="list" style:--columns={gridTemplate(columns)}>
    {#each rows as row, nth (ordinal === 'index' ? row.id : `${nth}:${row.id}`)}
      <button
        class="row"
        class:current={row.isCurrent}
        ondblclick={() => onactivate(row)}
        oncontextmenu={(event) => oncontext?.(event, row)}
      >
        {#each defs as def (def.id)}
          <span class="cell kind-{def.kind}">{def.value(row, ordinal === 'index' ? row.index : nth)}</span>
        {/each}
      </button>
    {/each}
  </div>
{/if}

<style>
  .list {
    display: flex;
    flex-direction: column;
  }
  /* Строка один в один как в плейлисте: тот же кегль, те же отступы
     ячеек и никаких промежутков между колонками. Иначе в поиске шрифт
     становился крупнее, а колонки уезжали относительно заголовка. */
  .row {
    display: grid;
    grid-template-columns: var(--columns);
    align-items: center;
    height: var(--row-h);
    padding: 0;
    font: inherit;
    font-size: var(--fs-sm);
    text-align: left;
    color: var(--fg);
    background: none;
    border: none;
    cursor: pointer;
  }
  .row:nth-child(even) {
    background: var(--row-alt);
  }
  .row:hover {
    background: var(--row-hover);
  }
  .row.current .kind-title {
    color: var(--accent);
    font-weight: 600;
  }
  .cell {
    padding: 0 8px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .kind-mono {
    color: var(--fg-faint);
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    text-align: right;
  }
  .kind-dim {
    color: var(--fg-dim);
  }
  .kind-stars {
    font-size: var(--fs-xs);
    color: color-mix(in srgb, var(--accent) 80%, var(--fg-dim));
    text-align: right;
  }
  .empty {
    margin: 0;
    padding: 10px 2px;
    color: var(--fg-faint);
    font-size: var(--fs-xs);
  }
</style>
