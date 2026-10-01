<script module lang="ts">
  export type MenuItem =
    /** `danger` — действие, которое меняет файлы на диске: оно должно
        выделяться среди безобидных, а не стоять в общем ряду. */
    | { label: string; action: () => void; disabled?: boolean; danger?: boolean }
    | { separator: true };
</script>

<script lang="ts">
  let {
    items,
    x,
    y,
    onclose,
  }: { items: MenuItem[]; x: number; y: number; onclose: () => void } = $props();

  let menu: HTMLDivElement | null = $state(null);
  let width = $state(0);
  let height = $state(0);

  // Меню не должно уезжать за край окна: у нижних строк списка это ровно тот
  // случай, когда оно вылезает целиком.
  const left = $derived(Math.max(4, Math.min(x, window.innerWidth - width - 4)));
  const top = $derived(Math.max(4, Math.min(y, window.innerHeight - height - 4)));

  $effect(() => {
    if (!menu) return;
    width = menu.offsetWidth;
    height = menu.offsetHeight;
    menu.focus();
  });

  function pick(item: MenuItem) {
    if ('separator' in item || item.disabled) return;
    onclose();
    item.action();
  }
</script>

<svelte:window
  onresize={onclose}
  onkeydown={(event) => {
    if (event.key === 'Escape') onclose();
  }}
/>

<!-- Подложка ловит клик мимо меню и правый клик по другой строке. -->
<div
  class="scrim"
  role="presentation"
  onpointerdown={onclose}
  oncontextmenu={(event) => {
    event.preventDefault();
    onclose();
  }}
></div>

<div
  class="menu"
  role="menu"
  tabindex="-1"
  bind:this={menu}
  style:left="{left}px"
  style:top="{top}px"
>
  {#each items as item, index (index)}
    {#if 'separator' in item}
      <div class="sep"></div>
    {:else}
      <button
        role="menuitem"
        class:danger={item.danger}
        disabled={item.disabled}
        onclick={() => pick(item)}
      >
        {item.label}
      </button>
    {/if}
  {/each}
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 40;
  }
  .menu {
    position: fixed;
    z-index: 41;
    min-width: 190px;
    padding: 4px;
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow-pop);
    outline: none;
  }
  button {
    display: block;
    width: 100%;
    text-align: left;
    font: inherit;
    color: var(--fg);
    background: none;
    border: none;
    border-radius: calc(var(--radius) - 2px);
    padding: 5px 10px;
    cursor: pointer;
    white-space: nowrap;
  }
  button:hover:not(:disabled) {
    background: var(--accent);
    color: var(--accent-fg);
  }
  button:disabled {
    color: var(--fg-faint);
    cursor: default;
  }
  button.danger:not(:disabled) {
    color: var(--danger);
  }
  button.danger:hover:not(:disabled) {
    background: var(--danger);
    color: #fff;
  }
  .sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--border);
  }
</style>
