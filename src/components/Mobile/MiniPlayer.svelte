<script lang="ts">
  import { player } from '../../stores/player.svelte.ts';
  import Icon from './Icon.svelte';

  /**
   * Полоска «сейчас играет» над нижней панелью: видна из любого раздела,
   * касание открывает плеер во весь экран.
   */
  const { onopen }: { onopen: () => void } = $props();

  const playing = $derived(player.status === 'playing');
  const artist = $derived(player.tags?.artist ?? '');
  const progress = $derived(
    player.durationMs > 0 ? Math.min(1, player.positionMs / player.durationMs) : 0,
  );
</script>

{#if player.track}
  <div class="mini">
    <div class="progress" style:--progress={progress}></div>
    <button class="open" aria-label="Открыть плеер" onclick={onopen}>
      <span class="cover">
        {#if player.coverUrl}
          <img src={player.coverUrl} alt="" />
        {:else}
          <Icon name={player.isStream ? 'radio' : 'note'} size={20} />
        {/if}
      </span>
      <span class="text">
        <span class="title">{player.title}</span>
        {#if artist}<span class="artist">{artist}</span>{/if}
      </span>
    </button>
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
  .open {
    flex: 1;
    min-width: 0;
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
