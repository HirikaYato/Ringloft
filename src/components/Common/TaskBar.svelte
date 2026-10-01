<script lang="ts">
  /**
   * Полоса фоновой задачи: что делаем, сколько сделано, над чем работаем и
   * кнопка «Остановить». Таких задач уже две (громкость и тексты), а вид у
   * них один — держать его в двух файлах значит дать ему разъехаться.
   */
  let {
    label,
    running,
    processed = 0,
    total = 0,
    current = null,
    note = '',
    failed = false,
    onstop,
    onclose,
  }: {
    label: string;
    running: boolean;
    processed?: number;
    total?: number;
    current?: string | null;
    /** Итог, когда работа кончилась. */
    note?: string;
    failed?: boolean;
    onstop?: () => void;
    onclose?: () => void;
  } = $props();

  const percent = $derived(total > 0 ? Math.round((processed / total) * 100) : 0);
</script>

{#if running}
  <div class="task">
    <div class="line">
      <span class="what">{label}</span>
      {#if total > 0}
        <span class="counts">{Math.min(processed + 1, total)} из {total}</span>
      {/if}
      <span class="file">{current ?? ''}</span>
      {#if onstop}<button class="ghost" onclick={onstop}>Остановить</button>{/if}
    </div>
    <div class="bar"><div class="fill" style:width="{percent}%"></div></div>
  </div>
{:else if note}
  <div class="task">
    <div class="line">
      <span class="what" class:failed>{note}</span>
      {#if onclose}<button class="ghost" onclick={onclose} title="Скрыть">×</button>{/if}
    </div>
  </div>
{/if}

<style>
  .task {
    padding: 6px 10px;
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    font-size: var(--fs-sm);
  }
  .line {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .what {
    flex: 0 0 auto;
  }
  .what.failed {
    color: var(--danger);
  }
  .counts {
    flex: 0 0 auto;
    color: var(--fg-dim);
    font-variant-numeric: tabular-nums;
  }
  /* Имя файла отдаёт место остальному: путь длиннее полосы — обычное дело. */
  .file {
    flex: 1 1 auto;
    min-width: 0;
    color: var(--fg-faint);
    font-size: var(--fs-xs);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .line button {
    flex: 0 0 auto;
    margin-left: auto;
  }
  .bar {
    height: 3px;
    margin-top: 6px;
    border-radius: 2px;
    background: var(--bg-inset);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    transition: width 120ms linear;
  }
</style>
