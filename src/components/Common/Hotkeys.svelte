<script lang="ts">
  /** Шпаргалка по клавишам. Рисуется из той же таблицы, что и обработчик. */
  import { HOTKEYS, HOTKEY_GROUPS, LIST_KEYS } from '../../lib/hotkeys';
</script>

<div class="sheet">
  {#each HOTKEY_GROUPS as group (group)}
    <section>
      <h3>{group}</h3>
      <dl>
        {#each HOTKEYS.filter((hotkey) => hotkey.group === group) as hotkey (hotkey.id)}
          <dt><kbd>{hotkey.keys}</kbd></dt>
          <dd>{hotkey.label}</dd>
        {/each}
        {#if group === HOTKEY_GROUPS[2]}
          {#each LIST_KEYS as item (item.keys)}
            <dt><kbd>{item.keys}</kbd></dt>
            <dd>{item.label}</dd>
          {/each}
        {/if}
      </dl>
    </section>
  {/each}
  <p class="hint">
    Клавиши работают, когда окно в фокусе и курсор не в поле ввода. Системные
    медиаклавиши идут через рабочий стол, а не через плеер; если он их не
    отдаёт, любое сочетание можно повесить на команду
    <code>ringloft --play-pause</code> (а также <code>--next</code>,
    <code>--prev</code>, <code>--stop</code>) в настройках рабочего стола.
  </p>
</div>

<style>
  /* Три группы в колонки: списком в одну строку шпаргалка выходит на два экрана. */
  .sheet {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: 18px 24px;
    align-items: start;
  }
  h3 {
    margin: 0 0 8px;
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--fg-dim);
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 6px 10px;
    margin: 0;
    align-items: baseline;
  }
  dt {
    justify-self: end;
  }
  dd {
    margin: 0;
    font-size: var(--fs-sm);
  }
  kbd {
    display: inline-block;
    padding: 1px 6px;
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    border-radius: 5px;
    background: var(--bg-inset);
    color: var(--fg);
    /* Моноширинный здесь только мешал: стрелки в нём мельче букв и на
       11 пикселях читались как точки. */
    font-size: var(--fs-sm);
    white-space: nowrap;
  }
  .hint {
    grid-column: 1 / -1;
    margin: 0;
    color: var(--fg-faint);
    font-size: var(--fs-xs);
    line-height: 1.5;
  }
  code {
    font-family: var(--font-mono);
    color: var(--fg-dim);
  }
</style>
