<script lang="ts">
  import { formatTime } from '../../lib/format';
  import type { RepeatMode } from '../../lib/api';
  import { player } from '../../stores/player.svelte.ts';
  import { settings } from '../../stores/settings.svelte.ts';
  import SleepButton from './SleepButton.svelte';

  let barEl: HTMLDivElement | null = $state(null);
  let volEl: HTMLDivElement | null = $state(null);
  let volDragging = $state(false);
  let dragging = $state(false);
  let dragMs = $state(0);
  /** Перемотка сбрасывает кольцо и декодер, поэтому при таскании шлём не чаще 8 раз в секунду. */
  let lastSentAt = 0;

  const shownMs = $derived(dragging ? dragMs : player.positionMs);
  const progress = $derived(
    player.durationMs > 0 ? Math.min(100, (shownMs / player.durationMs) * 100) : 0,
  );
  const playing = $derived(player.status === 'playing');
  const hasTrack = $derived(player.track !== null);
  const seekable = $derived(hasTrack && player.durationMs > 0);
  const repeat = $derived(settings.current?.playback.repeat ?? 'off');
  const shuffle = $derived(settings.current?.playback.shuffle ?? false);

  /** off → все → один → off: как в AIMP, одной кнопкой по кругу. */
  const REPEAT_TITLE: Record<RepeatMode, string> = {
    off: 'Повтор выключен',
    all: 'Повтор плейлиста',
    track: 'Повтор трека',
  };

  function msFromEvent(event: PointerEvent): number {
    if (!barEl) return 0;
    const rect = barEl.getBoundingClientRect();
    const ratio = Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width));
    return ratio * player.durationMs;
  }

  function pointerDown(event: PointerEvent) {
    if (!seekable) return;
    barEl?.setPointerCapture(event.pointerId);
    dragging = true;
    dragMs = msFromEvent(event);
    lastSentAt = performance.now();
    void player.seek(dragMs);
  }

  function pointerMove(event: PointerEvent) {
    if (!dragging) return;
    dragMs = msFromEvent(event);
    if (performance.now() - lastSentAt > 120) {
      lastSentAt = performance.now();
      void player.seek(dragMs);
    }
  }

  function pointerUp(event: PointerEvent) {
    if (!dragging) return;
    dragging = false;
    barEl?.releasePointerCapture(event.pointerId);
    void player.seek(msFromEvent(event));
  }

  function volumeFromEvent(event: PointerEvent): number {
    if (!volEl) return 0;
    const rect = volEl.getBoundingClientRect();
    return Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width));
  }

  function volDown(event: PointerEvent) {
    volEl?.setPointerCapture(event.pointerId);
    volDragging = true;
    void player.setVolume(volumeFromEvent(event));
  }

  function volMove(event: PointerEvent) {
    if (volDragging) void player.setVolume(volumeFromEvent(event));
  }

  function volUp(event: PointerEvent) {
    if (!volDragging) return;
    volDragging = false;
    volEl?.releasePointerCapture(event.pointerId);
  }

  function volKeyDown(event: KeyboardEvent) {
    const step = event.shiftKey ? 0.1 : 0.05;
    if (event.key === 'ArrowLeft' || event.key === 'ArrowDown') {
      event.preventDefault();
      void player.setVolume(Math.max(0, player.volume - step));
    } else if (event.key === 'ArrowRight' || event.key === 'ArrowUp') {
      event.preventDefault();
      void player.setVolume(Math.min(1, player.volume + step));
    }
  }

  function keyDown(event: KeyboardEvent) {
    if (!seekable) return;
    const step = event.shiftKey ? 30_000 : 5_000;
    if (event.key === 'ArrowLeft') {
      event.preventDefault();
      void player.seek(Math.max(0, player.positionMs - step));
    } else if (event.key === 'ArrowRight') {
      event.preventDefault();
      void player.seek(Math.min(player.durationMs, player.positionMs + step));
    }
  }
</script>

<div class="transport">
  <button class="icon" title="Предыдущий" onclick={() => player.prev()}>
    <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M12 3.2v9.6c0 .6-.7 1-1.2.6l-6.4-4.8a.8.8 0 0 1 0-1.2l6.4-4.8c.5-.4 1.2 0 1.2.6z" /><rect x="3" y="3" width="1.6" height="10" rx=".8" /></svg>
  </button>

  <button
    class="icon primary"
    disabled={!hasTrack}
    title={playing ? 'Пауза' : 'Воспроизведение'}
    onclick={() => player.toggle()}
  >
    {#if playing}
      <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="3.5" y="2.5" width="3.5" height="11" rx="1" /><rect x="9" y="2.5" width="3.5" height="11" rx="1" /></svg>
    {:else}
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 2.8v10.4c0 .7.8 1.2 1.4.8l8-5.2c.6-.4.6-1.2 0-1.6l-8-5.2c-.6-.4-1.4 0-1.4.8z" /></svg>
    {/if}
  </button>

  <button class="icon" title="Следующий" onclick={() => player.next()}>
    <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 3.2v9.6c0 .6.7 1 1.2.6l6.4-4.8a.8.8 0 0 0 0-1.2L5.2 2.6c-.5-.4-1.2 0-1.2.6z" /><rect x="11.4" y="3" width="1.6" height="10" rx=".8" /></svg>
  </button>

  <button class="icon" disabled={!hasTrack} title="Стоп" onclick={() => player.stop()}>
    <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="3" y="3" width="10" height="10" rx="1.5" /></svg>
  </button>

  <button
    class="icon"
    class:active={repeat !== 'off'}
    title={REPEAT_TITLE[repeat]}
    onclick={() => void settings.cycleRepeat()}
  >
    <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4.5 4.5h7a2 2 0 0 1 2 2v1M11.5 11.5h-7a2 2 0 0 1-2-2v-1" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" /><path d="m10 2.6 1.9 1.9L10 6.4M6 9.6 4.1 11.5 6 13.4" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" /></svg>
    {#if repeat === 'track'}<span class="badge-one">1</span>{/if}
  </button>

  <button
    class="icon"
    class:active={shuffle}
    title={shuffle ? 'Перемешивание включено' : 'Перемешивание выключено'}
    onclick={() => settings.setShuffle(!shuffle)}
  >
    <svg viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M2.5 4.5h2.2l6.1 7h2.7M2.5 11.5h2.2l6.1-7h2.7" /><path d="m11.8 2.6 1.9 1.9-1.9 1.9M11.8 9.6l1.9 1.9-1.9 1.9" /></svg>
  </button>

  <SleepButton />

  <span class="time">{formatTime(shownMs)}</span>

  <div
    class="bar"
    class:seekable
    bind:this={barEl}
    role="slider"
    tabindex="0"
    aria-label="Позиция"
    aria-valuemin="0"
    aria-valuemax={player.durationMs}
    aria-valuenow={Math.round(shownMs)}
    aria-valuetext={formatTime(shownMs)}
    onpointerdown={pointerDown}
    onpointermove={pointerMove}
    onpointerup={pointerUp}
    onpointercancel={pointerUp}
    onkeydown={keyDown}
  >
    <div class="track"><div class="fill" style:width="{progress}%"></div></div>
    <div class="handle" style:left="{progress}%"></div>
  </div>

  <span class="time dim">{formatTime(player.durationMs)}</span>

  <div class="volume">
    <button
      class="vol-icon"
      title={player.volume > 0 ? 'Выключить звук' : 'Включить звук'}
      onclick={() => void player.toggleMute()}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M7.2 2.4 4 5H2.2c-.4 0-.7.3-.7.7v4.6c0 .4.3.7.7.7H4l3.2 2.6c.5.4 1.1 0 1.1-.6V3c0-.6-.6-1-1.1-.6z" />
        {#if player.volume === 0}
          <path d="M10.4 6.2 13.6 9.4M13.6 6.2l-3.2 3.2" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
        {:else}
          <path d="M10.6 5.2a.7.7 0 0 0-.2 1 3 3 0 0 1 0 3.6.7.7 0 1 0 1.1.8 4.4 4.4 0 0 0 0-5.2.7.7 0 0 0-.9-.2z" />
          {#if player.volume > 0.55}
            <path d="M12.7 3.2a.7.7 0 0 0-.3 1 5.4 5.4 0 0 1 0 7.6.7.7 0 1 0 1 1 6.8 6.8 0 0 0 0-9.6.7.7 0 0 0-.7-.1z" />
          {/if}
        {/if}
      </svg>
    </button>
    <div
      class="vbar"
      bind:this={volEl}
      role="slider"
      tabindex="0"
      aria-label="Громкость"
      aria-valuemin="0"
      aria-valuemax="100"
      aria-valuenow={Math.round(player.volume * 100)}
      title="Громкость {Math.round(player.volume * 100)}%"
      onpointerdown={volDown}
      onpointermove={volMove}
      onpointerup={volUp}
      onpointercancel={volUp}
      onkeydown={volKeyDown}
    >
      <div class="track"><div class="fill" style:width="{player.volume * 100}%"></div></div>
      <div class="handle" style:left="{player.volume * 100}%"></div>
    </div>
  </div>
</div>

<style>
  /* Своей подложки у транспорта нет: он живёт внутри нижней панели
     (`PlayerBar`) и внутри полноэкранного режима, и фон рисуют они. */
  .transport {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .icon {
    position: relative;
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 50%;
    background: var(--bg-inset);
    color: var(--fg-dim);
    cursor: pointer;
    transition:
      background var(--dur-fast) var(--ease),
      color var(--dur-fast) var(--ease);
  }
  .icon svg {
    width: 14px;
    height: 14px;
    fill: currentColor;
  }
  .icon:hover:not(:disabled) {
    background: var(--row-hover);
    color: var(--fg);
  }
  .icon:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .icon.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-fg);
  }
  .icon.primary:hover:not(:disabled) {
    filter: brightness(1.1);
  }
  .icon.active {
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 50%, var(--border));
  }
  .badge-one {
    position: absolute;
    right: 2px;
    bottom: 1px;
    font-size: 9px;
    font-weight: 700;
    line-height: 1;
    color: var(--accent);
  }
  .time {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    min-width: 42px;
    text-align: center;
  }
  .time.dim {
    color: var(--fg-faint);
  }
  .bar {
    position: relative;
    flex: 1;
    height: 18px;
    display: flex;
    align-items: center;
    touch-action: none;
  }
  .bar.seekable {
    cursor: pointer;
  }
  .bar:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: 2px;
    border-radius: 3px;
  }
  .track {
    width: 100%;
    height: 4px;
    border-radius: 2px;
    background: var(--bg-inset);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent);
  }
  .handle {
    position: absolute;
    top: 50%;
    width: 10px;
    height: 10px;
    margin-left: -5px;
    border-radius: 50%;
    background: var(--accent);
    transform: translateY(-50%) scale(0);
    transition: transform var(--dur-fast) var(--ease);
  }
  .bar.seekable:hover .handle,
  .bar:focus-visible .handle {
    transform: translateY(-50%) scale(1);
  }
  .volume {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 128px;
    flex: none;
  }
  .vol-icon {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    padding: 0;
    color: var(--fg-dim);
    background: none;
    border: none;
    border-radius: var(--radius);
    cursor: pointer;
  }
  .vol-icon:hover {
    color: var(--fg);
    background: var(--row-hover);
  }
  .vol-icon svg {
    width: 15px;
    height: 15px;
    fill: currentColor;
  }
  /* Регулятор нарисован теми же деталями, что и полоса позиции: два разных
     по виду ползунка в одной строке выглядели чужеродно. */
  .vbar {
    position: relative;
    flex: 1;
    height: 18px;
    display: flex;
    align-items: center;
    cursor: pointer;
    touch-action: none;
  }
  .vbar:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: 2px;
    border-radius: 3px;
  }
  .vbar .fill {
    background: var(--fg-dim);
  }
  .vbar:hover .fill {
    background: var(--accent);
  }
</style>
