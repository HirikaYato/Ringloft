<script lang="ts">
  import { formatTime } from '../../lib/format';
  import { player } from '../../stores/player.svelte.ts';
  import Cover from './Cover.svelte';

  /**
   * Строка трека для списков телефона: обложка, название, вторая строка и
   * время. Высота фиксированная — списки виртуальные и считают от неё.
   *
   * Долгое нажатие — отдельный жест (`onhold`), а касание после него уже не
   * считается: иначе трек, который хотели отметить, тут же бы заиграл.
   */
  const {
    path,
    title,
    subtitle = '',
    durationMs = null,
    current = false,
    selected = false,
    onpress,
    onhold,
  }: {
    path: string;
    title: string;
    subtitle?: string;
    durationMs?: number | null;
    current?: boolean;
    selected?: boolean;
    onpress: () => void;
    onhold?: () => void;
  } = $props();

  const HOLD_MS = 450;
  /** Палец поехал дальше этого — это прокрутка, а не нажатие. */
  const SLOP = 10;

  let timer: ReturnType<typeof setTimeout> | null = null;
  let held = false;
  let startX = 0;
  let startY = 0;

  function cancel() {
    if (timer) clearTimeout(timer);
    timer = null;
  }

  function down(event: PointerEvent) {
    held = false;
    if (!onhold) return;
    startX = event.clientX;
    startY = event.clientY;
    cancel();
    timer = setTimeout(() => {
      timer = null;
      held = true;
      navigator.vibrate?.(12);
      onhold();
    }, HOLD_MS);
  }

  function move(event: PointerEvent) {
    if (!timer) return;
    if (Math.abs(event.clientX - startX) > SLOP || Math.abs(event.clientY - startY) > SLOP) cancel();
  }

  function click() {
    if (held) {
      held = false;
      return;
    }
    onpress();
  }
</script>

<button
  class="row"
  class:current
  class:selected
  onpointerdown={down}
  onpointermove={move}
  onpointerup={cancel}
  onpointercancel={cancel}
  onclick={click}
  oncontextmenu={(event) => event.preventDefault()}
>
  <span class="art">
    <Cover {path} />
    <!-- Играющая строка видна и без цвета названия: полоски на обложке, на
         паузе они замирают. -->
    {#if current}
      <span class="bars" class:paused={player.status !== 'playing'} aria-hidden="true">
        <i></i><i></i><i></i>
      </span>
    {/if}
  </span>
  <span class="text">
    <span class="title">{title}</span>
    {#if subtitle}<span class="subtitle">{subtitle}</span>{/if}
  </span>
  {#if durationMs}<span class="time">{formatTime(durationMs)}</span>{/if}
</button>

<style>
  .row {
    width: 100%;
    height: var(--m-row, 60px);
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0 16px;
    font: inherit;
    text-align: left;
    color: var(--fg);
    background: none;
    border: none;
    /* Долгое нажатие — наш жест, а не выделение текста и не системное меню. */
    user-select: none;
    -webkit-touch-callout: none;
  }
  .row:active {
    background: var(--row-hover);
  }
  .row.selected {
    background: var(--accent-soft);
  }
  .art {
    position: relative;
    flex: none;
    display: grid;
  }
  .bars {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: flex-end;
    justify-content: center;
    gap: 3px;
    padding-bottom: 12px;
    border-radius: 6px;
    background: rgb(0 0 0 / 0.45);
  }
  .bars i {
    width: 4px;
    height: 18px;
    border-radius: 1px;
    background: var(--accent);
    transform-origin: bottom;
    animation: bounce 900ms ease-in-out infinite alternate;
  }
  .bars i:nth-child(2) {
    animation-delay: -300ms;
    animation-duration: 700ms;
  }
  .bars i:nth-child(3) {
    animation-delay: -600ms;
    animation-duration: 1100ms;
  }
  .bars.paused i {
    animation-play-state: paused;
  }
  @keyframes bounce {
    from {
      transform: scaleY(0.25);
    }
    to {
      transform: scaleY(1);
    }
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .title,
  .subtitle {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .subtitle {
    font-size: var(--fs-sm);
    color: var(--fg-faint);
  }
  .current .title {
    color: var(--accent);
    font-weight: 600;
  }
  .time {
    flex: none;
    font-size: var(--fs-sm);
    font-variant-numeric: tabular-nums;
    color: var(--fg-faint);
  }
</style>
