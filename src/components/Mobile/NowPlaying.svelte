<script lang="ts">
  import { onDestroy } from 'svelte';

  import { api } from '../../lib/api';
  import { pushBack } from '../../lib/back';
  import { formatTime } from '../../lib/format';
  import { player } from '../../stores/player.svelte.ts';
  import { playlist } from '../../stores/playlist.svelte.ts';
  import { settings } from '../../stores/settings.svelte.ts';
  import { sleep } from '../../stores/sleep.svelte.ts';
  import Equalizer from '../Player/Equalizer.svelte';
  import HeadphoneEq from '../Player/HeadphoneEq.svelte';
  import Lyrics from '../Player/Lyrics.svelte';
  import Icon from './Icon.svelte';
  import Sheet from './Sheet.svelte';
  import SheetItem from './SheetItem.svelte';
  import TrackRow from './TrackRow.svelte';

  /**
   * Плеер во весь экран: обложка, перемотка, транспорт. Текст песни и
   * очередь занимают место обложки, а не открываются поверх — так транспорт
   * остаётся под рукой, пока читаешь текст.
   *
   * Громкости здесь нет намеренно: на телефоне её крутят кнопками корпуса.
   */
  const { onclose }: { onclose: () => void } = $props();

  let gone = false;
  const close = pushBack(() => {
    if (!gone) onclose();
  });
  onDestroy(() => {
    gone = true;
    close();
  });

  let view = $state<'cover' | 'lyrics' | 'queue'>('cover');
  let sheet = $state<'none' | 'sleep' | 'eq'>('none');

  const playing = $derived(player.status === 'playing');
  const repeat = $derived(settings.current?.playback.repeat ?? 'off');
  const shuffle = $derived(settings.current?.playback.shuffle ?? false);
  const artist = $derived(player.tags?.artist ?? '');
  const album = $derived(player.tags?.album ?? '');
  const seekable = $derived(player.durationMs > 0);

  /** Пока ползунок тащат, показываем его место, а не позицию трека. */
  let dragging = $state<number | null>(null);
  const shown = $derived(dragging ?? player.positionMs);

  function seek() {
    if (dragging === null) return;
    void player.seek(dragging);
    dragging = null;
  }

  // Порядок вперёд зависит от трека, перемешивания и повтора.
  $effect(() => {
    if (view !== 'queue') return;
    void player.track?.itemId;
    void shuffle;
    void repeat;
    void playlist.loadUpcoming();
  });

  const SLEEP_MINUTES = [15, 30, 45, 60, 90];
  const sleepLabel = $derived(
    sleep.kind === 'timer'
      ? `Пауза через ${formatTime(sleep.remainingMs)}`
      : sleep.kind === 'afterTrack'
        ? 'Остановится после этого трека'
        : 'Таймер сна',
  );

  /** Смахнули вниз — закрыли. Жест ловим на шапке и обложке, а не на тексте. */
  let swipeFrom: { x: number; y: number } | null = null;
  function swipeEnd(event: PointerEvent) {
    const from = swipeFrom;
    swipeFrom = null;
    if (!from) return;
    const dy = event.clientY - from.y;
    if (dy > 90 && dy > Math.abs(event.clientX - from.x) * 1.5) close();
  }
</script>

<div class="now" role="dialog" aria-modal="true" aria-label="Сейчас играет">
  <div
    class="swipe"
    role="presentation"
    onpointerdown={(event) => (swipeFrom = { x: event.clientX, y: event.clientY })}
    onpointerup={swipeEnd}
    onpointercancel={() => (swipeFrom = null)}
  >
    <header>
      <button class="key" aria-label="Свернуть" onclick={close}><Icon name="down" /></button>
    </header>

    {#if view === 'cover'}
      <div class="art">
        {#if player.coverUrl}
          <img src={player.coverUrl} alt="" />
        {:else}
          <Icon name={player.isStream ? 'radio' : 'note'} size={72} />
        {/if}
      </div>
    {/if}
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

  <div class="seek">
    <input
      type="range"
      aria-label="Позиция"
      min="0"
      max={Math.max(1, player.durationMs)}
      value={shown}
      disabled={!seekable}
      oninput={(event) => (dragging = event.currentTarget.valueAsNumber)}
      onchange={seek}
    />
    <div class="times">
      <span>{formatTime(shown)}</span>
      <span>{seekable ? formatTime(player.durationMs) : 'эфир'}</span>
    </div>
  </div>

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

  <div class="extras">
    <button class="key" class:on={view === 'lyrics'} aria-label="Текст песни" aria-pressed={view === 'lyrics'} onclick={() => (view = view === 'lyrics' ? 'cover' : 'lyrics')}>
      <Icon name="lyrics" />
    </button>
    <button class="key" class:on={view === 'queue'} aria-label="Дальше по порядку" aria-pressed={view === 'queue'} onclick={() => (view = view === 'queue' ? 'cover' : 'queue')}>
      <Icon name="queue" />
    </button>
    <button class="key" class:on={sleep.kind !== 'off'} aria-label={sleepLabel} onclick={() => (sheet = 'sleep')}>
      <Icon name="moon" />
    </button>
    <button class="key" class:on={settings.current?.audio.eqEnabled} aria-label="Эквалайзер" onclick={() => (sheet = 'eq')}>
      <Icon name="eq" />
    </button>
  </div>
</div>

{#if sheet === 'sleep'}
  <Sheet title={sleepLabel} onclose={() => (sheet = 'none')}>
    {#snippet children(dismiss)}
      {#each SLEEP_MINUTES as minutes (minutes)}
        <SheetItem
          label={minutes < 60 ? `Через ${minutes} минут` : minutes === 60 ? 'Через час' : 'Через полтора часа'}
          onclick={() => {
            void sleep.set({ kind: 'minutes', minutes });
            dismiss();
          }}
        />
      {/each}
      <!-- У радио конца нет — останавливаться не по чему. -->
      <SheetItem
        label="После этого трека"
        active={sleep.kind === 'afterTrack'}
        disabled={player.isStream}
        onclick={() => {
          void sleep.set({ kind: 'afterTrack' });
          dismiss();
        }}
      />
      {#if sleep.kind !== 'off'}
        <SheetItem
          icon="close"
          label="Выключить таймер"
          onclick={() => {
            void sleep.set({ kind: 'off' });
            dismiss();
          }}
        />
      {/if}
    {/snippet}
  </Sheet>
{/if}

{#if sheet === 'eq'}
  <Sheet title="Эквалайзер" onclose={() => (sheet = 'none')}>
    {#snippet children()}
      <div class="eq">
        <Equalizer />
        <HeadphoneEq />
      </div>
    {/snippet}
  </Sheet>
{/if}

<style>
  .now {
    position: fixed;
    inset: 0;
    z-index: 25;
    display: flex;
    flex-direction: column;
    padding: env(safe-area-inset-top) 20px calc(env(safe-area-inset-bottom) + 12px);
    background: var(--bg);
    animation: rise var(--dur-mid) var(--ease);
  }
  @keyframes rise {
    from {
      transform: translateY(32px);
      opacity: 0;
    }
  }
  .swipe {
    display: flex;
    flex-direction: column;
    /* Вертикальный жест здесь наш — иначе вебвью примет его за прокрутку. */
    touch-action: none;
  }
  .swipe:has(.art) {
    flex: 1;
    min-height: 0;
  }
  header {
    display: flex;
    margin: 4px -12px 0;
  }
  .art {
    flex: 1;
    min-height: 0;
    display: grid;
    place-items: center;
    margin: 8px 0 16px;
    color: var(--fg-faint);
    /* Обложка — квадрат во всю ширину, но не выше отведённого места:
       считаем от размеров самого блока, а не от картинки. */
    container-type: size;
  }
  .art img {
    width: min(100cqw, 100cqh);
    height: min(100cqw, 100cqh);
    object-fit: cover;
    border-radius: 14px;
    box-shadow: var(--shadow-pop);
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
  .seek input {
    width: 100%;
    height: 32px;
    margin: 4px 0 0;
  }
  .times {
    display: flex;
    justify-content: space-between;
    font-size: var(--fs-sm);
    font-variant-numeric: tabular-nums;
    color: var(--fg-faint);
  }
  .transport,
  .extras {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .transport {
    padding: 8px 0 4px;
  }
  .extras {
    justify-content: space-around;
    padding-top: 4px;
  }
  .key {
    position: relative;
    width: 52px;
    height: 52px;
    display: grid;
    place-items: center;
    color: var(--fg-dim);
    background: none;
    border: none;
    border-radius: 50%;
  }
  .transport .key {
    color: var(--fg);
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
  }
  .one {
    position: absolute;
    right: 9px;
    bottom: 9px;
    font-size: 10px;
    font-weight: 700;
    line-height: 1;
  }
  .eq {
    padding: 0 16px 16px;
  }
</style>
