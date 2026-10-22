<script lang="ts">
  import { onDestroy } from 'svelte';

  import { api } from '../../lib/api';
  import { pushBack } from '../../lib/back';
  import { flung, swipe } from '../../lib/swipe';
  import { player } from '../../stores/player.svelte.ts';
  import { playlist } from '../../stores/playlist.svelte.ts';
  import { settings } from '../../stores/settings.svelte.ts';
  import { sleep } from '../../stores/sleep.svelte.ts';
  import Lyrics from '../Player/Lyrics.svelte';
  import CoverDeck from './CoverDeck.svelte';
  import Icon from './Icon.svelte';
  import PlayerSheets, { sleepLabel, type PlayerSheet } from './PlayerSheets.svelte';
  import SeekBar from './SeekBar.svelte';
  import Transport from './Transport.svelte';
  import TrackRow from './TrackRow.svelte';

  /**
   * Плеер во весь экран: обложка, перемотка, транспорт. Текст песни и
   * очередь занимают место обложки, а не открываются поверх — так транспорт
   * остаётся под рукой, пока читаешь текст.
   *
   * Громкости здесь нет намеренно: на телефоне её крутят кнопками корпуса.
   */
  const { onclose, onreveal }: { onclose: () => void; onreveal: () => void } = $props();

  const LEAVE_MS = 220;

  let height = $state(0);
  /** Насколько плеер стянули вниз пальцем. */
  let pull = $state(0);
  let pullMs = $state(0);
  let gone = false;
  let leaving = false;

  /** Уезжает вниз и только потом исчезает — и от кнопки, и от «назад». */
  function leave() {
    if (leaving || gone) return;
    leaving = true;
    pullMs = LEAVE_MS;
    pull = height;
    setTimeout(() => {
      if (!gone) onclose();
    }, LEAVE_MS);
  }

  const close = pushBack(leave);
  onDestroy(() => {
    gone = true;
    close();
  });

  let view = $state<'cover' | 'lyrics' | 'queue'>('cover');
  let sheet = $state<PlayerSheet>('none');

  const repeat = $derived(settings.current?.playback.repeat ?? 'off');
  const shuffle = $derived(settings.current?.playback.shuffle ?? false);
  const artist = $derived(player.tags?.artist ?? '');
  const album = $derived(player.tags?.album ?? '');
  const source = $derived(playlist.tabs.find((tab) => tab.isPlaying));

  // Порядок вперёд зависит от трека, перемешивания и повтора.
  $effect(() => {
    if (view !== 'queue') return;
    void player.track?.itemId;
    void shuffle;
    void repeat;
    void playlist.loadUpcoming();
  });

  /** Показать играющий трек в его списке: плеер уезжает, список встаёт на строку. */
  function reveal() {
    close();
    onreveal();
  }
</script>

<div
  class="now"
  class:tinted={player.coverTint !== null}
  class:pulled={pull > 0}
  role="dialog"
  aria-modal="true"
  aria-label="Сейчас играет"
  bind:clientHeight={height}
  style:--tint={player.coverTint ?? 'transparent'}
  style:transform="translateY({pull}px)"
  style:transition="transform {pullMs}ms var(--ease)"
>
  <!-- Вниз тянут за шапку и обложку; текст и очередь прокручиваются сами. -->
  <div
    class="grip"
    class:full={view === 'cover'}
    {@attach swipe({
      axes: () => ({ y: !leaving }),
      move: (_axis, delta) => {
        pullMs = 0;
        pull = Math.max(0, delta);
      },
      end: (_axis, delta, velocity) => {
        if (delta > 0 && flung(delta, velocity, height, 0.25)) {
          close();
        } else {
          pullMs = LEAVE_MS;
          pull = 0;
        }
      },
    })}
  >
    <header>
      <button class="key" aria-label="Свернуть" onclick={close}><Icon name="down" /></button>
      {#if source}
        <button class="source" onclick={reveal}>
          <span class="label">{source.isTemporary ? 'Играет' : 'Играет из списка'}</span>
          <span class="name">{source.name}</span>
        </button>
      {/if}
    </header>

    {#if view === 'cover'}<CoverDeck />{/if}
  </div>

  {#if view === 'lyrics'}
    <div class="pane"><Lyrics big /></div>
  {:else if view === 'queue'}
    <div class="pane queue">
      {#each playlist.upcoming as row (row.id)}
        <TrackRow
          path={row.path}
          title={row.title}
          subtitle={row.artist ?? ''}
          durationMs={row.durationMs}
          onpress={() => void api.playlistPlayItem(row.id)}
        />
      {:else}
        <p class="note">Дальше ничего нет: это последний трек, а повтор выключен.</p>
      {/each}
    </div>
  {/if}

  <div class="meta">
    <h1>{player.title || 'Ничего не играет'}</h1>
    {#if artist || album}<p>{[artist, album].filter(Boolean).join(' — ')}</p>{/if}
  </div>

  <SeekBar />

  <Transport />

  <div class="extras">
    <button class="key" class:on={view === 'lyrics'} aria-label="Текст песни" aria-pressed={view === 'lyrics'} onclick={() => (view = view === 'lyrics' ? 'cover' : 'lyrics')}>
      <Icon name="lyrics" />
    </button>
    <button class="key" class:on={view === 'queue'} aria-label="Дальше по порядку" aria-pressed={view === 'queue'} onclick={() => (view = view === 'queue' ? 'cover' : 'queue')}>
      <Icon name="queue" />
    </button>
    <button class="key" class:on={sleep.kind !== 'off'} aria-label={sleepLabel()} onclick={() => (sheet = 'sleep')}>
      <Icon name="moon" />
    </button>
    <button class="key" class:on={settings.current?.audio.eqEnabled} aria-label="Эквалайзер" onclick={() => (sheet = 'eq')}>
      <Icon name="eq" />
    </button>
  </div>
</div>

<PlayerSheets bind:sheet />

<style>
  .now {
    position: fixed;
    inset: 0;
    z-index: 25;
    display: flex;
    flex-direction: column;
    padding: env(safe-area-inset-top) 20px calc(env(safe-area-inset-bottom) + 12px);
    background: var(--bg);
    box-shadow: 0 -8px 32px rgb(0 0 0 / 0.3);
    /* Пятно цвета обложки (ниже) лежит под содержимым, но над фоном. */
    isolation: isolate;
    animation: rise var(--dur-mid) var(--ease);
  }
  @keyframes rise {
    from {
      transform: translateY(32px);
      opacity: 0;
    }
  }
  /* Пятно цвета обложки сверху: экран перестаёт быть одинаковым для любого
     трека. Цвет меняется плавно — фон перекрашивается, а не мигает. */
  .now::before {
    content: '';
    position: absolute;
    inset: 0 0 35% 0;
    z-index: -1;
    background: var(--tint);
    opacity: 0;
    mask-image: linear-gradient(to bottom, black, transparent);
    transition:
      background-color 600ms var(--ease),
      opacity 600ms var(--ease);
    pointer-events: none;
  }
  .now.tinted::before {
    opacity: 0.45;
  }
  /* Стянутый вниз плеер — лист поверх приложения, у листа скруглён верх. */
  .now.pulled {
    border-radius: 18px 18px 0 0;
    overflow: hidden;
  }
  .grip {
    display: flex;
    flex-direction: column;
    /* Вертикальный жест здесь наш — иначе вебвью примет его за прокрутку. */
    touch-action: none;
  }
  .grip.full {
    flex: 1;
    min-height: 0;
  }
  header {
    display: grid;
    grid-template-columns: 52px 1fr 52px;
    align-items: center;
    margin: 4px -12px 0;
  }
  .source {
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1px;
    padding: 6px 8px;
    font: inherit;
    color: var(--fg);
    background: none;
    border: none;
    border-radius: var(--radius-lg);
  }
  .source:active {
    background: var(--row-hover);
  }
  .label {
    font-size: var(--fs-xs);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--fg-faint);
  }
  .name {
    max-width: 100%;
    font-size: var(--fs-sm);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pane {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    margin: 0 -20px;
  }
  /* Строка текста шириной 100 % плюс свои поля шире колонки на эти поля: на
     широком окне десктопа этого не видно, а на телефоне текст уезжал за
     правый край. Поля считаем внутрь и оставляем запас под увеличение
     активной строки. */
  .pane :global(.lyrics.big .line) {
    box-sizing: border-box;
    padding-inline: 8%;
  }
  .queue {
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  .note {
    margin: 40px 28px;
    text-align: center;
    color: var(--fg-faint);
  }
  .meta {
    padding: 8px 0 4px;
  }
  h1 {
    margin: 0;
    font-size: 22px;
    font-weight: 600;
    line-height: 1.25;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .meta p {
    margin: 4px 0 0;
    color: var(--fg-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .extras {
    display: flex;
    align-items: center;
    justify-content: space-around;
    padding-top: 4px;
  }
  .key {
    width: 52px;
    height: 52px;
    display: grid;
    place-items: center;
    color: var(--fg-dim);
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
</style>
