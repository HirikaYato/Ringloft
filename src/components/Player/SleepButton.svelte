<script lang="ts">
  /** Луна рядом с повтором: таймер сна и «остановить после этого трека». */
  import ContextMenu, { type MenuItem } from '../Common/ContextMenu.svelte';
  import { formatTime } from '../../lib/format';
  import { player } from '../../stores/player.svelte.ts';
  import { sleep } from '../../stores/sleep.svelte.ts';

  let menu = $state<{ x: number; y: number } | null>(null);

  const MINUTES = [15, 30, 45, 60, 90];
  const isStream = $derived(player.track?.path.startsWith('http') ?? false);

  const title = $derived.by(() => {
    if (sleep.kind === 'timer') return `Таймер сна: пауза через ${formatTime(sleep.remainingMs)}`;
    if (sleep.kind === 'afterTrack') return 'Остановится после этого трека';
    return 'Таймер сна';
  });

  const items = $derived<MenuItem[]>([
    ...MINUTES.map((minutes) => ({
      label: minutes < 60 ? `Через ${minutes} минут` : minutes === 60 ? 'Через час' : 'Через полтора часа',
      action: () => void sleep.set({ kind: 'minutes', minutes }),
    })),
    {
      label: 'После этого трека',
      // У радио конца нет — останавливаться не по чему.
      disabled: isStream,
      action: () => void sleep.set({ kind: 'afterTrack' }),
    },
    { separator: true },
    { label: 'Выключить таймер', disabled: sleep.kind === 'off', action: () => void sleep.set({ kind: 'off' }) },
  ]);

  function open(event: MouseEvent) {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    // Кнопка внизу окна — меню раскрываем вверх от неё.
    menu = { x: rect.left, y: rect.top - 4 - (items.length * 30 + 12) };
  }
</script>

<button
  class="icon sleep"
  class:active={sleep.kind !== 'off'}
  {title}
  aria-label={title}
  onclick={open}
>
  <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M13.2 9.6A5.6 5.6 0 0 1 6.4 2.8a5.6 5.6 0 1 0 6.8 6.8z" /></svg>
  {#if sleep.kind === 'afterTrack'}<span class="badge">1</span>{/if}
</button>
{#if sleep.kind === 'timer'}
  <span class="left" title={title}>{formatTime(sleep.remainingMs)}</span>
{/if}

{#if menu}
  <ContextMenu items={items} x={menu.x} y={menu.y} onclose={() => (menu = null)} />
{/if}

<style>
  /* Вид кнопки — тот же, что у соседних в транспорте. */
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
  .icon:hover {
    background: var(--row-hover);
    color: var(--fg);
  }
  .icon.active {
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 50%, var(--border));
  }
  .icon svg {
    width: 14px;
    height: 14px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.4;
    stroke-linejoin: round;
  }
  .icon.active svg {
    fill: currentColor;
  }
  .badge {
    position: absolute;
    right: 2px;
    bottom: 1px;
    font-size: 9px;
    font-weight: 700;
    line-height: 1;
    color: var(--accent);
  }
  .left {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--accent);
  }
</style>
