<script lang="ts">
  import { formatTime } from '../../lib/format';
  import { player } from '../../stores/player.svelte.ts';

  /**
   * Перемотка в плеере во весь экран. Полоса своя, а не системная: у
   * системной в вебвью нет заливки пройденного, и позицию трека с одного
   * взгляда было не понять.
   */
  const seekable = $derived(player.durationMs > 0);

  /** Пока ползунок тащат, показываем его место, а не позицию трека. */
  let dragging = $state<number | null>(null);
  const shown = $derived(dragging ?? player.positionMs);
  const share = $derived(seekable ? Math.min(1, shown / player.durationMs) : 0);

  function seek() {
    if (dragging === null) return;
    void player.seek(dragging);
    dragging = null;
  }
</script>

<div class="seek" class:dragging={dragging !== null}>
  <input
    type="range"
    aria-label="Позиция"
    min="0"
    max={Math.max(1, player.durationMs)}
    value={shown}
    disabled={!seekable}
    style:--share="{share * 100}%"
    oninput={(event) => (dragging = event.currentTarget.valueAsNumber)}
    onchange={seek}
  />
  <div class="times">
    <span>{formatTime(shown)}</span>
    <span>{seekable ? formatTime(player.durationMs) : 'эфир'}</span>
  </div>
</div>

<style>
  input {
    width: 100%;
    height: 32px;
    margin: 4px 0 0;
    padding: 0;
    background: none;
    appearance: none;
    -webkit-appearance: none;
  }
  input::-webkit-slider-runnable-track {
    height: 4px;
    border-radius: 2px;
    background: linear-gradient(
      to right,
      var(--accent) var(--share),
      color-mix(in srgb, var(--fg) 16%, transparent) var(--share)
    );
  }
  input::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 14px;
    height: 14px;
    margin-top: -5px;
    border-radius: 50%;
    background: var(--fg);
    border: none;
    transition: transform var(--dur-fast) var(--ease);
  }
  .dragging input::-webkit-slider-thumb {
    transform: scale(1.35);
  }
  /* У радио перемотки нет: полоса пустая, ползунка нет. */
  input:disabled::-webkit-slider-thumb {
    visibility: hidden;
  }
  .times {
    display: flex;
    justify-content: space-between;
    font-size: var(--fs-sm);
    font-variant-numeric: tabular-nums;
    color: var(--fg-faint);
  }
  .dragging .times span:first-child {
    color: var(--fg);
  }
</style>
