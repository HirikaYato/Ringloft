<script lang="ts">
  import { swipe } from '../../lib/swipe';
  import { TrackFling } from '../../lib/track-fling.svelte.ts';
  import { player } from '../../stores/player.svelte.ts';
  import Icon from './Icon.svelte';

  /**
   * Полоска «сейчас играет» над нижней панелью: видна из любого раздела.
   * Касание или жест вверх открывает плеер во весь экран, жест вбок
   * листает треки — как обложка в самом плеере.
   */
  const { onopen }: { onopen: () => void } = $props();

  const playing = $derived(player.status === 'playing');
  const artist = $derived(player.tags?.artist ?? '');
  const progress = $derived(
    player.durationMs > 0 ? Math.min(1, player.positionMs / player.durationMs) : 0,
  );

  let width = $state(0);
  const fling = new TrackFling(() => width);
</script>

{#if player.track}
  <div
    class="mini"
    class:tinted={player.coverTint !== null}
    style:--tint={player.coverTint ?? 'transparent'}
  >
    <div class="progress" style:--progress={progress}></div>
    <div
      class="lane"
      bind:clientWidth={width}
      {@attach swipe({
        axes: () => ({ x: !fling.busy, y: true }),
        move: (axis, delta) => {
          if (axis === 'x') fling.drag(delta);
        },
        end: (axis, delta, velocity) => {
          if (axis === 'x') void fling.drop(delta, velocity);
          else if (delta < -24 || velocity < -0.3) onopen();
        },
      })}
    >
      <button class="open" aria-label="Открыть плеер" style={fling.style} onclick={onopen}>
        <span class="cover">
          {#if player.coverUrl}
            <img src={player.coverUrl} alt="" draggable="false" />
          {:else}
            <Icon name={player.isStream ? 'radio' : 'note'} size={20} />
          {/if}
        </span>
        <span class="text">
          <span class="title">{player.title}</span>
          {#if artist}<span class="artist">{artist}</span>{/if}
        </span>
      </button>
    </div>
    <button class="key" aria-label={playing ? 'Пауза' : 'Играть'} onclick={() => player.toggle()}>
      <Icon name={playing ? 'pause' : 'play'} size={24} />
    </button>
    <button class="key" aria-label="Следующий" onclick={() => player.next()}>
      <Icon name="next" size={24} />
    </button>
  </div>
{/if}

<style>
  .mini {
    position: relative;
    flex: none;
    display: flex;
    align-items: center;
    padding: 0 6px 0 12px;
    background: var(--bg-elev);
    border-top: 1px solid var(--border);
    transition: background-color 600ms var(--ease);
  }
  .mini.tinted {
    background: color-mix(in srgb, var(--tint) 14%, var(--bg-elev));
  }
  /* Позиция — тонкой линией по верхнему краю: полоса перемотки сюда не
     помещается, а без неё не видно, идёт ли трек. */
  .progress {
    position: absolute;
    left: 0;
    top: -1px;
    height: 2px;
    width: 100%;
    background: var(--accent);
    transform-origin: left;
    transform: scaleX(var(--progress));
  }
  /* Полоса, по которой уезжает карточка трека: за её краем карточку не
     видно, кнопки справа стоят на месте. */
  .lane {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    touch-action: none;
  }
  .open {
    width: 100%;
    min-height: 60px;
    display: flex;
    align-items: center;
    gap: 12px;
    font: inherit;
    text-align: left;
    color: var(--fg);
    background: none;
    border: none;
    padding: 0;
  }
  .cover {
    flex: none;
    width: 44px;
    height: 44px;
    display: grid;
    place-items: center;
    overflow: hidden;
    border-radius: 6px;
    color: var(--fg-faint);
    background: var(--bg-inset);
  }
  .cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .text {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .title,
  .artist {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .artist {
    font-size: var(--fs-sm);
    color: var(--fg-faint);
  }
  .key {
    flex: none;
    width: 48px;
    height: 48px;
    display: grid;
    place-items: center;
    color: var(--fg);
    background: none;
    border: none;
    border-radius: 50%;
  }
  .key:active {
    background: var(--row-hover);
  }
</style>
