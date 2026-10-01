<script lang="ts">
  import type { Snippet } from 'svelte';

  /**
   * Модальное окно: настройки и эквалайзер — это отдельный разговор, а не
   * ещё одна полка над плейлистом. Полками они отъедали треть окна и всё
   * равно не помещались.
   */
  let {
    title,
    onclose,
    width = 560,
    children,
  }: { title: string; onclose: () => void; width?: number; children: Snippet } = $props();

  let dialog: HTMLDivElement | null = $state(null);

  $effect(() => dialog?.focus());
</script>

<svelte:window
  onkeydown={(event) => {
    if (event.key === 'Escape') onclose();
  }}
/>

<div class="scrim" role="presentation" onpointerdown={onclose}></div>

<div
  class="dialog"
  role="dialog"
  aria-modal="true"
  aria-label={title}
  tabindex="-1"
  bind:this={dialog}
  style:--dialog-width="{width}px"
>
  <header>
    <h1>{title}</h1>
    <button class="close" title="Закрыть (Esc)" onclick={onclose}>
      <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M3 3l6 6M9 3l-6 6" /></svg>
    </button>
  </header>
  <div class="body">{@render children()}</div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 20;
    background: var(--scrim);
    animation: fade-in var(--dur-mid) var(--ease);
  }
  .dialog {
    position: fixed;
    z-index: 21;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: min(var(--dialog-width), calc(100vw - 32px));
    max-height: min(680px, calc(100vh - 64px));
    display: flex;
    flex-direction: column;
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-pop);
    outline: none;
    animation: pop-in var(--dur-mid) var(--ease);
  }
  @keyframes fade-in {
    from {
      opacity: 0;
    }
  }
  @keyframes pop-in {
    from {
      opacity: 0;
      transform: translate(-50%, calc(-50% + 8px));
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .scrim,
    .dialog {
      animation: none;
    }
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 14px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  h1 {
    margin: 0;
    font-size: var(--fs-lg);
    font-weight: 600;
  }
  .close {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    color: var(--fg-dim);
    background: none;
    border: none;
    border-radius: var(--radius);
    cursor: pointer;
  }
  .close:hover {
    background: var(--row-hover);
    color: var(--fg);
  }
  .close svg {
    width: 12px;
    height: 12px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.3;
    stroke-linecap: round;
  }
  .body {
    min-height: 0;
    overflow: auto;
    padding: 4px 14px 14px;
  }
</style>
