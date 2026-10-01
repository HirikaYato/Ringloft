<script lang="ts" generics="T">
  import type { Snippet } from 'svelte';

  /**
   * Список с фиксированной высотой строки: в DOM живут только видимые строки.
   * Тот же приём, что в плейлисте, но без выделения и перетаскивания — для
   * списков библиотеки этого достаточно.
   */
  const {
    items,
    rowHeight = 30,
    pad = 6,
    row,
  }: {
    items: T[];
    rowHeight?: number;
    /** Сколько строк дорисовываем за краями экрана. */
    pad?: number;
    row: Snippet<[T, number]>;
  } = $props();

  let scrollTop = $state(0);
  let height = $state(0);

  const first = $derived(Math.max(0, Math.floor(scrollTop / rowHeight) - pad));
  const last = $derived(
    Math.min(items.length, Math.ceil((scrollTop + height) / rowHeight) + pad),
  );
  const visible = $derived(
    Array.from({ length: Math.max(0, last - first) }, (_, nth) => first + nth),
  );
</script>

<div
  class="viewport"
  bind:clientHeight={height}
  onscroll={(event) => (scrollTop = event.currentTarget.scrollTop)}
>
  <div class="spacer" style:height="{items.length * rowHeight}px">
    {#each visible as index (index)}
      <div class="row" style:top="{index * rowHeight}px" style:height="{rowHeight}px">
        {@render row(items[index] as T, index)}
      </div>
    {/each}
  </div>
</div>

<style>
  .viewport {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
  }
  .spacer {
    position: relative;
  }
  .row {
    position: absolute;
    left: 0;
    right: 0;
  }
</style>
