<script lang="ts">
  /** Пауза в песне (и долгое вступление): три точки «дышат», пока она идёт. */
  let { index, active }: { index: number; active: boolean } = $props();
</script>

<p class="pause" class:active data-line={index} aria-hidden="true">
  <span></span><span></span><span></span>
</p>

<style>
  .pause {
    display: flex;
    gap: 0.35em;
    align-items: center;
    justify-content: var(--pause-align, flex-start);
    height: 1.4em;
    margin: 0;
    padding: 0 0.1em;
    opacity: 0.2;
    transition: opacity 300ms var(--ease);
  }
  .pause span {
    width: 0.32em;
    height: 0.32em;
    border-radius: 50%;
    background: currentColor;
  }
  .pause.active {
    opacity: 0.85;
  }
  /* Анимация только у идущей паузы: остальные стоят и не тратят кадры. */
  .pause.active span {
    animation: breathe 1.5s var(--ease) infinite;
  }
  .pause.active span:nth-child(2) {
    animation-delay: 0.2s;
  }
  .pause.active span:nth-child(3) {
    animation-delay: 0.4s;
  }
  @keyframes breathe {
    0%,
    100% {
      transform: scale(0.7);
      opacity: 0.5;
    }
    50% {
      transform: scale(1.15);
      opacity: 1;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .pause.active span {
      animation: none;
    }
  }
</style>
