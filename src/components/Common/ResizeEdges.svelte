<script lang="ts">
  import { appWindow } from '../../lib/window';

  /**
   * Окно без системной рамки теряет и системные края для растягивания:
   * на Wayland их рисует декоратор, которого у нас больше нет. Поэтому
   * восемь невидимых полосок по периметру, каждая просит у Tauri тянуть
   * окно в свою сторону.
   */
  const EDGES = [
    ['North', 'n'],
    ['South', 's'],
    ['East', 'e'],
    ['West', 'w'],
    ['NorthEast', 'ne'],
    ['NorthWest', 'nw'],
    ['SouthEast', 'se'],
    ['SouthWest', 'sw'],
  ] as const;

  const win = appWindow();

  function grab(event: PointerEvent, direction: (typeof EDGES)[number][0]) {
    if (event.button !== 0 || !win) return;
    event.preventDefault();
    void win.startResizeDragging(direction);
  }
</script>

{#each win ? EDGES : [] as [direction, side] (side)}
  <div
    class="edge {side}"
    role="presentation"
    onpointerdown={(event) => grab(event, direction)}
  ></div>
{/each}

<style>
  .edge {
    position: fixed;
    z-index: 60;
  }
  .n,
  .s {
    left: 6px;
    right: 6px;
    height: 4px;
    cursor: ns-resize;
  }
  .e,
  .w {
    top: 6px;
    bottom: 6px;
    width: 4px;
    cursor: ew-resize;
  }
  .n { top: 0; }
  .s { bottom: 0; }
  .e { right: 0; }
  .w { left: 0; }
  .ne,
  .nw,
  .se,
  .sw {
    width: 10px;
    height: 10px;
  }
  .ne { top: 0; right: 0; cursor: nesw-resize; }
  .nw { top: 0; left: 0; cursor: nwse-resize; }
  .se { bottom: 0; right: 0; cursor: nwse-resize; }
  .sw { bottom: 0; left: 0; cursor: nesw-resize; }
</style>
