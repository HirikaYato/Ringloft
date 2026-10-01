<script lang="ts">
  import { LyricsScroller } from '../../lib/lyrics-scroller';
  import { activeLine, lineLength, LONG_PAUSE_MS, nextStamp } from '../../lib/lyrics-timing';
  import { player } from '../../stores/player.svelte.ts';
  import LyricsPause from './LyricsPause.svelte';
  import LyricsSearch from './LyricsSearch.svelte';

  /** `big` — вид для полноэкранного режима: крупнее, по центру и с
      растворением строк по краям. Полкой над плейлистом столько места нет. */
  let { big = false }: { big?: boolean } = $props();

  // Текст лежит в тегах или в `.lrc` рядом и читается лениво; панель открыта —
  // значит, пора.
  $effect(() => {
    if (player.hasLyrics) void player.loadLyrics();
  });

  const lyrics = $derived(player.lyrics);
  const lines = $derived(lyrics?.lines ?? []);
  const synced = $derived(lyrics?.synced ?? false);

  /** Что нашлось — чтобы было видно, тот ли это трек. */
  const foundNote = $derived.by(() => {
    if (player.lyricsSearch !== 'found' || !player.lyricsMatched) return '';
    const saved = player.lyricsSaved ? ', сохранил рядом с треком' : '';
    return `Нашёл: ${player.lyricsMatched}${saved}`;
  });

  /**
   * Подсветка чуть опережает звук: глазу нужно мгновение, чтобы перескочить
   * на строку, а переход заметен уже в первые 150 мс. Без опережения строка
   * загоралась, когда её уже начали петь.
   */
  const LEAD_MS = 150;

  /**
   * Позиция для подсветки — по своим часам. Раньше строка менялась только с
   * приходом тика, а тики идут раз в 66 мс и неровно: переход то и дело
   * запаздывал. Теперь таймер заводится ровно на метку следующей строки, а
   * каждый тик (и пауза, и перемотка) просто перезаводит его.
   */
  let clock = $state(0);

  $effect(() => {
    if (!synced) return;
    const list = lines;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const step = () => {
      const now = player.livePositionMs() + LEAD_MS;
      clock = now;
      if (player.status !== 'playing') return;
      const next = nextStamp(list, activeLine(list, now));
      if (next !== null) timer = setTimeout(step, next - now + 1);
    };
    step();
    return () => clearTimeout(timer);
  });

  const active = $derived(synced ? activeLine(lines, clock) : -1);
  /** Долгое вступление показываем точками, как и паузы посреди песни. */
  const intro = $derived(synced && (lines[0]?.atMs ?? 0) >= LONG_PAUSE_MS);

  function isLongPause(index: number): boolean {
    return lineLength(lines, index, player.durationMs) >= LONG_PAUSE_MS;
  }

  let box: HTMLDivElement | null = $state(null);
  let track: HTMLDivElement | null = $state(null);
  let boxHeight = $state(0);
  let boxWidth = $state(0);
  let scroller: LyricsScroller | null = null;

  $effect(() => {
    if (!box || !track) return;
    const created = new LyricsScroller(box, track);
    scroller = created;
    return () => {
      created.destroy();
      scroller = null;
    };
  });

  /** Новый текст доводим до места сразу, а не проезжаем его целиком. */
  let shownLines: typeof lines | null = null;

  // Активная строка к середине. Смена ширины и высоты тоже повод: строки
  // переносятся по-другому, и середина уезжает.
  $effect(() => {
    const index = active;
    void boxHeight;
    void boxWidth;
    if (!track || !scroller) return;
    if (!synced || lines.length === 0) {
      scroller.reset();
      shownLines = null;
      return;
    }
    const jump = shownLines !== lines;
    shownLines = lines;
    scroller.follow(track.querySelector<HTMLElement>(`[data-line="${index}"]`), jump);
  });

  async function seekTo(index: number) {
    const at = lines[index]?.atMs;
    if (at === null || at === undefined) return;
    scroller?.release();
    await player.seek(at);
  }
</script>

<!-- Касание нужно только чтобы заметить, что текст читают руками: сама
     область ничего не делает, отсюда `presentation`. -->
<div
  class="lyrics"
  class:big
  class:synced
  role="presentation"
  bind:this={box}
  bind:clientHeight={boxHeight}
  bind:clientWidth={boxWidth}
  onpointerdown={() => scroller?.touch()}
>
  <div class="track" bind:this={track}>
  {#if !player.track}
    <p class="note">Ничего не играет.</p>
  {:else if !player.hasLyrics}
    <LyricsSearch />
  {:else if lyrics === null}
    <p class="note">Читаю текст…</p>
  {:else if lines.length === 0}
    <p class="note">Текст нашёлся, но он пустой.</p>
  {:else}
    {#if foundNote}<p class="found">{foundNote}</p>{/if}
    <!-- Отступы в полвысоты: первая и последняя строки тоже встают по
         центру, а не прилипают к краям. -->
    {#if synced}<div style:height="{boxHeight / 2}px"></div>{/if}
    {#if intro}<LyricsPause index={-1} active={active === -1} />{/if}
    {#each lines as line, index (index)}
      {#if line.text.trim() !== '' && synced}
        <!-- По синхронной строке можно попасть в нужное место трека. -->
        <button
          class="line"
          class:active={index === active}
          class:past={index < active}
          data-line={index}
          onclick={() => seekTo(index)}
        >
          {line.text}
        </button>
      {:else if line.text.trim() !== ''}
        <p class="line">{line.text}</p>
      {:else if synced && isLongPause(index)}
        <LyricsPause {index} active={index === active} />
      {:else}
        <p class="gap" data-line={index}></p>
      {/if}
    {/each}
    {#if synced}<div style:height="{boxHeight / 2}px"></div>{/if}
  {/if}
  </div>
</div>

<style>
  .lyrics {
    overflow-x: hidden;
    overflow-y: auto;
    /* Докрутили до конца — дальше колесо не должно листать то, что снаружи. */
    overscroll-behavior: contain;
    height: 100%;
    min-height: 0;
    /* Полосу прокрутки скрываем: панель ведёт себя сама. */
    scrollbar-width: none;
    font-family: var(--font-lyrics);
    font-size: var(--fs-lg);
    font-weight: 600;
    line-height: 1.4;
    overflow-wrap: anywhere;
    /* В приложении выделение текста отключено, но текст песни хочется
       скопировать — здесь возвращаем. */
    -webkit-user-select: text;
    user-select: text;
    cursor: text;
  }
  .lyrics::-webkit-scrollbar {
    display: none;
  }
  /* Синхронный текст едет сам (`lib/lyrics-scroller.ts`), родной прокрутки
     у него нет. `data-motion` ставит скрипт, поэтому для компилятора Svelte
     атрибут обёрнут в `:global` — иначе он выкинул бы правило как лишнее. */
  .lyrics.synced {
    overflow: hidden;
  }
  .track {
    position: relative;
    --lyrics-ease: cubic-bezier(0.22, 1, 0.36, 1);
  }
  .synced .track {
    will-change: transform;
  }
  .track:global([data-motion='follow']) {
    transition: transform 700ms var(--lyrics-ease);
  }
  /* Колесо — быстро, но не скачком: иначе текст дёргается на каждый щелчок. */
  .track:global([data-motion='manual']) {
    transition: transform 160ms ease-out;
  }
  /* Крупный вид: у краёв текст растворяется — иначе он выглядит обрезанным
     ножницами. Короткий несинхронный текст встаёт по центру (`safe` — чтобы у
     длинного не отрезало верх). */
  .lyrics.big {
    display: grid;
    align-content: safe center;
    font-size: clamp(calc(var(--fs-md) * 1.35), 3.2vh, calc(var(--fs-md) * 2.1));
    font-weight: 700;
    line-height: 1.3;
    text-align: center;
    mask-image: linear-gradient(to bottom, transparent, #000 12%, #000 88%, transparent);
    --pause-align: center;
  }
  .line {
    display: block;
    width: 100%;
    margin: 0;
    padding: 0.2em 0;
    font: inherit;
    text-align: inherit;
    color: var(--fg);
    background: none;
    border: none;
    white-space: pre-wrap;
  }
  .big .line {
    padding: 0.28em 5%;
  }
  /*
   * Акцент на текущей строке — прозрачностью и масштабом, а не жирностью:
   * жирная строка шире, переносится по-другому и сдвигает весь текст ровно в
   * момент перехода. `transform` раскладку не трогает.
   */
  .synced .line {
    cursor: pointer;
    opacity: 0.45;
    transform-origin: left center;
    /* Те же полсекунды с хвостом, что и у движения текста: подсветка и
       сдвиг заканчиваются вместе, а не наперегонки. */
    transition:
      opacity 450ms var(--lyrics-ease),
      transform 600ms var(--lyrics-ease),
      color 450ms var(--lyrics-ease);
  }
  .big.synced .line {
    transform-origin: center;
  }
  .synced .line.past {
    opacity: 0.28;
  }
  .synced .line.active {
    opacity: 1;
    color: var(--accent);
    transform: scale(1.04);
  }
  /* На обложке во весь экран акцентный цвет теряется — там строка белая. */
  .big.synced .line.active {
    color: var(--fg);
    transform: scale(1.06);
  }
  .synced .line:hover:not(.active) {
    opacity: 0.75;
  }
  .gap {
    height: 0.6em;
    margin: 0;
  }
  @media (prefers-reduced-motion: reduce) {
    .synced .line,
    .synced .line.active {
      transform: none;
    }
    .track:global([data-motion]) {
      transition: none;
    }
  }
  /* Служебные подписи — шрифтом интерфейса, а не песни. */
  .found,
  .note {
    margin: 0 0 8px;
    color: var(--fg-faint);
    font-family: var(--font-ui);
    font-size: var(--fs-xs);
    font-weight: 400;
    text-align: center;
  }
</style>
