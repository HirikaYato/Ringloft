<script lang="ts">
  import { onDestroy, onMount } from 'svelte';

  import { appWindow } from '../../lib/window';

  const win = appWindow();
  let maximized = $state(false);
  let unlisten: (() => void) | null = null;

  onMount(async () => {
    if (!win) return;
    try {
      maximized = await win.isMaximized();
      // Разворачивать окно можно и мимо наших кнопок (двойной клик по полосе,
      // клавиши KDE), поэтому следим за реальным размером.
      unlisten = await win.onResized(async () => {
        maximized = await win.isMaximized();
      });
    } catch (err) {
      console.warn('[ringloft] состояние окна недоступно', err);
    }
  });

  onDestroy(() => unlisten?.());
</script>

{#if win}
  <div class="buttons">
    <button title="Свернуть" onclick={() => win.minimize()}>
      <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M2.5 6h7" /></svg>
    </button>
    <button title={maximized ? 'Восстановить' : 'Развернуть'} onclick={() => win.toggleMaximize()}>
      {#if maximized}
        <svg viewBox="0 0 12 12" aria-hidden="true"><rect x="2.5" y="3.5" width="6" height="5" rx="1" /><path d="M4.5 3.5V3a1 1 0 0 1 1-1H9a1 1 0 0 1 1 1v3.5a1 1 0 0 1-1 1h-.5" /></svg>
      {:else}
        <svg viewBox="0 0 12 12" aria-hidden="true"><rect x="2.5" y="2.5" width="7" height="7" rx="1" /></svg>
      {/if}
    </button>
    <button class="close" title="Закрыть" onclick={() => win.close()}>
      <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M3 3l6 6M9 3l-6 6" /></svg>
    </button>
  </div>
{/if}

<style>
  .buttons {
    display: flex;
    gap: 2px;
    margin-left: 4px;
  }
  button {
    width: 30px;
    height: 26px;
    display: grid;
    place-items: center;
    color: var(--fg-dim);
    background: none;
    border: none;
    border-radius: var(--radius);
    cursor: pointer;
    transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
  }
  button:hover {
    background: var(--row-hover);
    color: var(--fg);
  }
  button.close:hover {
    background: var(--danger);
    color: #fff;
  }
  svg {
    width: 12px;
    height: 12px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.2;
    stroke-linecap: round;
  }
</style>
