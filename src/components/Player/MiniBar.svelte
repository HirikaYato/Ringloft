<script lang="ts">
  import { formatTime } from '../../lib/format';
  import { player } from '../../stores/player.svelte.ts';

  let {
    onexpand,
    pinned,
    onpin,
    oncontext,
  }: {
    onexpand: () => void;
    pinned: boolean;
    onpin: (value: boolean) => void;
    oncontext: (event: MouseEvent) => void;
  } = $props();

  const track = $derived(player.track);
  const tags = $derived(player.tags);
  const title = $derived(player.title || 'Ничего не играет');
  const artist = $derived(tags?.artist ?? tags?.albumArtist ?? '');
  const playing = $derived(player.status === 'playing');
  const progress = $derived(
    player.durationMs > 0 ? Math.min(100, (player.positionMs / player.durationMs) * 100) : 0,
  );

  let bar: HTMLDivElement | null = $state(null);

  function seek(event: PointerEvent) {
    if (!bar || player.durationMs <= 0) return;
    const rect = bar.getBoundingClientRect();
    const ratio = Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width));
    void player.seek(ratio * player.durationMs);
  }
</script>

<!-- Окно таскают за саму полосу: заголовка в этом режиме нет вовсе. -->
<div
  class="mini"
  role="toolbar"
  aria-label="Компактный плеер"
  tabindex="-1"
  data-tauri-drag-region
  oncontextmenu={oncontext}
>
  <button class="cover" title="Вернуть обычный вид" onclick={onexpand}>
    {#if player.coverUrl}
      <img src={player.coverUrl} alt="" />
    {:else}
      <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 18V6l10-2v12" /><circle cx="6.5" cy="18" r="2.5" /><circle cx="16.5" cy="16" r="2.5" /></svg>
    {/if}
  </button>

  <div class="meta" data-tauri-drag-region>
    <span class="title" data-tauri-drag-region>{title}</span>
    <span class="sub" data-tauri-drag-region>
      {artist}
      {#if track}<span class="time">{formatTime(player.positionMs)} / {formatTime(player.durationMs)}</span>{/if}
    </span>
  </div>

  <div class="controls">
    <button
      class="icon"
      class:pinned
      title={pinned ? 'Отпустить окно' : 'Поверх всех окон'}
      aria-pressed={pinned}
      onclick={() => onpin(!pinned)}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M8 10.5V14M5 2.5h6l-.7 4.2 2 2.3H3.7l2-2.3z" /></svg>
    </button>
    <button class="icon" title="Предыдущий" onclick={() => player.prev()}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M12 3.2v9.6c0 .6-.7 1-1.2.6l-6.4-4.8a.8.8 0 0 1 0-1.2l6.4-4.8c.5-.4 1.2 0 1.2.6z" /><rect x="3" y="3" width="1.6" height="10" rx=".8" /></svg>
    </button>
    <button class="icon primary" disabled={!track} title={playing ? 'Пауза' : 'Играть'} onclick={() => player.toggle()}>
      {#if playing}
        <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="3.5" y="2.5" width="3.5" height="11" rx="1" /><rect x="9" y="2.5" width="3.5" height="11" rx="1" /></svg>
      {:else}
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 2.8v10.4c0 .7.8 1.2 1.4.8l8-5.2c.6-.4.6-1.2 0-1.6l-8-5.2c-.6-.4-1.4 0-1.4.8z" /></svg>
      {/if}
    </button>
    <button class="icon" title="Следующий" onclick={() => player.next()}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 3.2v9.6c0 .6.7 1 1.2.6l6.4-4.8a.8.8 0 0 0 0-1.2L5.2 2.6c-.5-.4-1.2 0-1.2.6z" /><rect x="11.4" y="3" width="1.6" height="10" rx=".8" /></svg>
    </button>
  </div>

  <!-- Вместо трёх кнопок окна одна: свернуть и закрыть есть в панели задач
       и в меню по правому клику, а вот вернуться к обычному виду больше
       негде. -->
  <button class="icon back" title="Обычный режим" onclick={onexpand}>
    <svg viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M2.5 6V2.5H6M10 2.5h3.5V6M13.5 10v3.5H10M6 13.5H2.5V10" /></svg>
  </button>

  <!-- Полоса позиции вдоль нижнего края: отдельной строки на неё нет. -->
  <div class="seek" bind:this={bar} role="presentation" onpointerdown={seek}>
    <div class="fill" style:width="{progress}%"></div>
  </div>
</div>

<style>
  .mini {
    position: relative;
    height: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 6px 0 10px;
    background: var(--bg-elev);
  }
  .cover {
    width: 52px;
    height: 52px;
    flex: none;
    display: grid;
    place-items: center;
    padding: 0;
    overflow: hidden;
    border-radius: var(--radius);
    background: var(--bg-inset);
    border: 1px solid var(--border);
    cursor: pointer;
  }
  .cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .cover svg {
    width: 20px;
    height: 20px;
    fill: none;
    stroke: var(--fg-faint);
    stroke-width: 1.6;
  }
  .meta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .title,
  .sub {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* Окно здесь 430×84 при любом шрифте, поэтому размер ограничен сверху:
     крупный шрифт из настроек в две строки высотой 84 не влезает. */
  .title {
    font-size: min(var(--fs-md), 16px);
    font-weight: 500;
  }
  .sub {
    font-size: min(var(--fs-xs), 13px);
    color: var(--fg-dim);
  }
  .time {
    margin-left: 6px;
    font-family: var(--font-mono);
    color: var(--fg-faint);
  }
  .controls {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: none;
  }
  .icon {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    padding: 0;
    color: var(--fg-dim);
    background: none;
    border: none;
    border-radius: 50%;
    cursor: pointer;
  }
  .icon svg {
    width: 13px;
    height: 13px;
    fill: currentColor;
  }
  /* Булавка нарисована контуром, а не заливкой. */
  .icon.pinned svg,
  .icon:first-child svg {
    fill: none;
  }
  .icon:hover:not(:disabled) {
    background: var(--row-hover);
    color: var(--fg);
  }
  .icon:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .icon.pinned {
    color: var(--accent);
    background: var(--accent-soft);
  }
  .icon.back {
    margin-left: 2px;
  }
  .icon.back svg {
    fill: none;
  }
  .icon.primary {
    width: 30px;
    height: 30px;
    background: var(--accent);
    color: var(--accent-fg);
  }
  .seek {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 5px;
    background: var(--bg-inset);
    cursor: pointer;
  }
  .fill {
    height: 100%;
    background: var(--accent);
  }
</style>
