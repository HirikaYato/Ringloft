<script lang="ts">
  import { player } from '../../stores/player.svelte.ts';
  import { settings } from '../../stores/settings.svelte.ts';
  import Icon from './Icon.svelte';

  /** Транспорт плеера во весь экран: перемешивание, назад, пуск, вперёд, повтор. */
  const playing = $derived(player.status === 'playing');
  const repeat = $derived(settings.current?.playback.repeat ?? 'off');
  const shuffle = $derived(settings.current?.playback.shuffle ?? false);
</script>

<div class="transport">
  <button class="key" class:on={shuffle} aria-label="Перемешивание" aria-pressed={shuffle} onclick={() => settings.setShuffle(!shuffle)}>
    <Icon name="shuffle" />
  </button>
  <button class="key" aria-label="Предыдущий" onclick={() => player.prev()}><Icon name="prev" size={30} /></button>
  <button class="key play" aria-label={playing ? 'Пауза' : 'Играть'} onclick={() => player.toggle()}>
    <Icon name={playing ? 'pause' : 'play'} size={30} />
  </button>
  <button class="key" aria-label="Следующий" onclick={() => player.next()}><Icon name="next" size={30} /></button>
  <button class="key repeat" class:on={repeat !== 'off'} aria-label="Повтор" onclick={() => void settings.cycleRepeat()}>
    <Icon name="repeat" />
    {#if repeat === 'track'}<span class="one">1</span>{/if}
  </button>
</div>

<style>
  .transport {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 0 4px;
  }
  .key {
    position: relative;
    width: 52px;
    height: 52px;
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
  .key.on {
    color: var(--accent);
  }
  .key.play {
    width: 68px;
    height: 68px;
    color: var(--accent-fg);
    background: var(--accent);
    transition: transform var(--dur-fast) var(--ease);
  }
  .key.play:active {
    transform: scale(0.94);
    background: var(--accent);
  }
  .one {
    position: absolute;
    right: 9px;
    bottom: 9px;
    font-size: 10px;
    font-weight: 700;
    line-height: 1;
  }
</style>
