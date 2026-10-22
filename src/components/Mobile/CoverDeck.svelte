<script lang="ts">
  import { untrack } from 'svelte';

  import { swipe } from '../../lib/swipe';
  import { coverReady, TrackFling } from '../../lib/track-fling.svelte.ts';
  import { player } from '../../stores/player.svelte.ts';
  import Icon from './Icon.svelte';

  /**
   * Большая обложка, которую листают пальцем: влево — следующий трек,
   * вправо — предыдущий.
   *
   * Картинку держим снимком, а не берём из стора напрямую: пока карточка
   * улетает, стор уже отозвал прежнюю обложку и грузит новую — без снимка
   * улетала бы пустая рамка.
   */
  type Card = { id: number; url: string | null; ready: boolean; stream: boolean };

  let width = $state(0);
  let card = $state<Card>(snapshot());
  /** Карточка въехала жестом — своё появление ей ни к чему. */
  let quiet = $state(false);

  const fling = new TrackFling(
    () => width,
    () => {
      quiet = true;
      card = snapshot();
    },
  );

  function snapshot(): Card {
    return {
      id: player.track?.itemId ?? -1,
      url: player.coverUrl,
      ready: coverReady(),
      stream: player.isStream,
    };
  }

  // В покое карточка следует за стором: трек сменился кнопкой или сам —
  // появляется заново; тот же трек дождался обложки — меняется картинка.
  $effect(() => {
    const next = snapshot();
    if (fling.busy) return;
    untrack(() => {
      if (next.id !== card.id) quiet = false;
      card = next;
    });
  });

  function appear(_node: Element, { quiet }: { quiet: boolean }) {
    if (quiet) return { duration: 0 };
    return {
      duration: 240,
      css: (t: number) => `opacity: ${t}; transform: scale(${0.94 + 0.06 * t})`,
    };
  }
</script>

<div
  class="deck"
  bind:clientWidth={width}
  {@attach swipe({
    axes: () => ({ x: !fling.busy && player.track !== null }),
    move: (_axis, delta) => fling.drag(delta),
    end: (_axis, delta, velocity) => void fling.drop(delta, velocity),
  })}
>
  <div class="slide" style={fling.style}>
    {#key card.id}
      <div class="card" class:blank={!card.url} in:appear={{ quiet }}>
        {#if card.url}
          <img src={card.url} alt="" draggable="false" />
        {:else if card.ready}
          <Icon name={card.stream ? 'radio' : 'note'} size={72} />
        {/if}
      </div>
    {/key}
  </div>
</div>

<style>
  .deck {
    flex: 1;
    min-height: 0;
    display: grid;
    place-items: center;
    margin: 8px 0 20px;
    /* Обе оси здесь наши: горизонталь листает, вертикаль закрывает плеер. */
    touch-action: none;
    /* Обложка — квадрат во всю ширину, но не выше отведённого места:
       считаем от размеров самого блока, а не от картинки. */
    container-type: size;
  }
  .slide {
    will-change: transform;
  }
  .card {
    width: min(100cqw, 100cqh);
    height: min(100cqw, 100cqh);
    display: grid;
    place-items: center;
    overflow: hidden;
    border-radius: 14px;
    box-shadow: var(--shadow-pop);
  }
  .card.blank {
    color: var(--fg-faint);
    background: var(--bg-inset);
    box-shadow: none;
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    animation: show var(--dur-mid) var(--ease);
  }
  @keyframes show {
    from {
      opacity: 0;
    }
  }
</style>
