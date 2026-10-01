<script lang="ts">
  import WindowButtons from '../Common/WindowButtons.svelte';
  import UpNext from '../Playlist/UpNext.svelte';
  import { player } from '../../stores/player.svelte.ts';
  import Lyrics from './Lyrics.svelte';
  import Spectrum from './Spectrum.svelte';
  import Transport from './Transport.svelte';

  let { onclose }: { onclose: () => void } = $props();

  /** Боковая колонка одна: текст и очередь делят одно место. */
  let side = $state<'none' | 'lyrics' | 'queue'>('none');
  let showSpectrum = $state(false);

  const VIEWS: { id: 'lyrics' | 'queue' | 'spectrum'; label: string; icon: string }[] = [
    {
      id: 'lyrics',
      label: 'Текст песни',
      icon: 'M8 1.8a2.2 2.2 0 0 1 2.2 2.2v3.4a2.2 2.2 0 0 1-4.4 0V4A2.2 2.2 0 0 1 8 1.8M4.2 7.2a3.8 3.8 0 0 0 7.6 0M8 11v3.2',
    },
    { id: 'queue', label: 'Что заиграет дальше', icon: 'M2.5 4h8M2.5 7.5h8M2.5 11h5M12 9.2l3.2 2-3.2 2z' },
    { id: 'spectrum', label: 'Спектр', icon: 'M2.5 13.5v-3M5.5 13.5v-7M8.5 13.5v-5M11.5 13.5V3M14 13.5v-4' },
  ];

  function toggleSide(next: 'lyrics' | 'queue') {
    side = side === next ? 'none' : next;
  }
  /** Колонка открыта — значит открыта. Раньше при отсутствии текста она
      просто не появлялась, и кнопка выглядела сломанной: а внутри как раз
      живёт поиск текста в интернете. */
  const split = $derived(side !== 'none');

  const tags = $derived(player.tags);
  const title = $derived(player.title || 'Ничего не играет');
  const artist = $derived(tags?.artist ?? tags?.albumArtist ?? null);
  const album = $derived(tags?.album ?? null);
  // Фон — та же обложка, что и в карточке: размытая и растянутая на всё окно.
  const background = $derived(player.coverUrl ? `url("${player.coverUrl}")` : 'none');
</script>

<svelte:window
  onkeydown={(event) => {
    if (event.key === 'Escape') onclose();
  }}
/>

<section class="full">
  <!-- Фон вылезает за края окна, поэтому живёт в своей обрезающей обёртке:
       иначе он добавляет оверлею прокрутку, и тот уезжает целиком. -->
  <div class="backdrop">
    <div class="bg" class:blank={!player.coverUrl} style:background-image={background}></div>
  </div>
  <div class="veil"></div>

  <!-- Окно таскают за эту полосу: системного заголовка нет, а оверлей
       перекрывает тот, что в главном окне. -->
  <div class="bar" data-tauri-drag-region>
    <button class="round" title="Свернуть (Esc)" aria-label="Свернуть" onclick={onclose}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 6 8 10.5 12.5 6" /></svg>
    </button>
    <span class="grow" data-tauri-drag-region></span>
    <!-- Виды — значками в одной капсуле: текстовые плашки поверх обложки
         смотрелись чужими. Подписи — во всплывающих подсказках. -->
    <div class="views" role="group" aria-label="Виды">
      {#each VIEWS as item (item.id)}
        <button
          class="view"
          class:active={item.id === 'spectrum' ? showSpectrum : side === item.id}
          title={item.label}
          aria-label={item.label}
          aria-pressed={item.id === 'spectrum' ? showSpectrum : side === item.id}
          onclick={() => (item.id === 'spectrum' ? (showSpectrum = !showSpectrum) : toggleSide(item.id))}
        >
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d={item.icon} /></svg>
        </button>
      {/each}
    </div>
    <!-- Полоса окна оверлеем перекрыта, поэтому кнопки окна живут и здесь. -->
    <span class="divider"></span>
    <WindowButtons />
  </div>

  <div class="stage" class:split>
    <div class="now">
      <div class="art">
        {#if player.coverUrl}
          <img src={player.coverUrl} alt="Обложка альбома" />
        {:else}
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 18V6l10-2v12" /><circle cx="6.5" cy="18" r="2.5" /><circle cx="16.5" cy="16" r="2.5" /></svg>
        {/if}
      </div>

      <div class="info">
        <h1>{title}</h1>
        {#if artist}<p class="artist">{artist}</p>{/if}
        {#if album}<p class="album">{album}{#if tags?.year}<span> · {tags.year}</span>{/if}</p>{/if}
      </div>
    </div>

    {#if split}
      <div class="side">
        {#if side === 'queue'}
          <UpNext />
        {:else}
          <Lyrics big />
        {/if}
      </div>
    {/if}
  </div>

  {#if showSpectrum}
    <div class="visual"><Spectrum bare /></div>
  {/if}

  <div class="controls"><Transport /></div>
</section>

<style>
  .full {
    position: fixed;
    inset: 0;
    z-index: 8;
    animation: fade-in var(--dur-mid) var(--ease);
    display: flex;
    flex-direction: column;
    background: var(--bg);
    overflow: hidden;
  }
  @keyframes fade-in {
    from {
      opacity: 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .full {
      animation: none;
    }
  }
  .backdrop {
    position: absolute;
    inset: 0;
    overflow: hidden;
  }
  .bg {
    position: absolute;
    /* Вылезаем за края: размытие иначе даёт светлую кайму по периметру. */
    inset: -8%;
    background-size: cover;
    background-position: center;
    filter: blur(48px) saturate(140%);
    transform: scale(1.1);
  }
  .bg.blank {
    background: radial-gradient(circle at 30% 20%, var(--accent), transparent 60%);
    opacity: 0.35;
  }
  .veil {
    position: absolute;
    inset: 0;
    background: linear-gradient(
      to bottom,
      color-mix(in srgb, var(--bg) 62%, transparent),
      color-mix(in srgb, var(--bg) 88%, transparent)
    );
  }
  .bar,
  .stage,
  .visual,
  .controls {
    position: relative;
  }
  .visual {
    flex: none;
    padding: 0 24px;
  }
  .divider {
    width: 1px;
    height: 18px;
    margin: 0 2px;
    background: var(--border);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 6px 8px 12px;
    flex: none;
  }
  .grow {
    flex: 1;
    align-self: stretch;
  }
  /* Значки поверх размытой обложки: полупрозрачная капсула и такие же
     круглые кнопки — в тон фону, а не плашками из главного окна. */
  .views {
    display: flex;
    gap: 2px;
    padding: 3px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--fg) 7%, transparent);
    border: 1px solid color-mix(in srgb, var(--fg) 9%, transparent);
    -webkit-backdrop-filter: blur(14px);
    backdrop-filter: blur(14px);
  }
  .view,
  .round {
    display: grid;
    place-items: center;
    padding: 0;
    color: color-mix(in srgb, var(--fg) 72%, transparent);
    background: none;
    border: 0;
    border-radius: 999px;
    cursor: pointer;
    transition:
      background var(--dur-fast) var(--ease),
      color var(--dur-fast) var(--ease);
  }
  .view {
    width: 36px;
    height: 30px;
  }
  .round {
    width: 34px;
    height: 34px;
    background: color-mix(in srgb, var(--fg) 7%, transparent);
    border: 1px solid color-mix(in srgb, var(--fg) 9%, transparent);
  }
  .view:hover,
  .round:hover {
    color: var(--fg);
    background: color-mix(in srgb, var(--fg) 12%, transparent);
  }
  .view.active {
    color: var(--fg);
    background: color-mix(in srgb, var(--accent) 38%, transparent);
  }
  .view svg,
  .round svg {
    width: 17px;
    height: 17px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.5;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .stage {
    flex: 1;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 40px;
    padding: 4px 40px 8px;
    text-align: center;
  }
  /* Открыта боковая колонка — обложка с подписями уходит влево одним блоком,
     а колонке достаётся вся высота сцены. Раньше текст начинался под
     заголовком, и под обложкой оставалась дыра в пол-экрана. */
  .stage.split {
    align-items: stretch;
    text-align: left;
  }
  .now {
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 22px;
  }
  .stage.split .now {
    flex: 0 0 auto;
    width: min(42vh, 38%);
    align-items: flex-start;
  }
  .art {
    width: min(46vh, 100%);
    aspect-ratio: 1;
    flex: none;
    display: grid;
    place-items: center;
    border-radius: 12px;
    overflow: hidden;
    background: var(--bg-inset);
    box-shadow: 0 18px 50px rgb(0 0 0 / 45%);
  }
  .stage.split .art {
    width: 100%;
  }
  .art img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .art svg {
    width: 28%;
    height: 28%;
    fill: none;
    stroke: var(--fg-faint);
    stroke-width: 1.2;
    stroke-linecap: round;
  }
  .info {
    min-width: 0;
    max-width: 760px;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .stage.split .info {
    max-width: 100%;
  }
  h1 {
    margin: 0;
    font-size: clamp(var(--fs-lg), 3.4vh, 30px);
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .artist {
    margin: 6px 0 0;
    font-size: var(--fs-lg);
    color: var(--fg-dim);
  }
  .album {
    margin: 2px 0 0;
    color: var(--fg-faint);
  }
  /* Боковая колонка: текст по центру и не шире удобной для чтения строки —
     на широком окне строка иначе тянется на всю ширину.

     Отступ сверху и снизу обязателен: без него первая строка подлезала под
     кнопки верхней полосы, а последняя — под транспорт. */
  .side {
    flex: 1 1 auto;
    min-width: 0;
    min-height: 0;
    padding-block: clamp(14px, 6vh, 64px);
    display: flex;
    flex-direction: column;
    align-items: center;
  }
  .side > :global(*) {
    width: 100%;
    max-width: 640px;
    min-height: 0;
    flex: 1;
  }
  /* Транспорт прижимался к самым краям окна: в главном окне отступы даёт
     нижняя панель, а здесь давать их некому. */
  .controls {
    flex: none;
    padding: 6px 20px 14px;
  }
</style>
