<script lang="ts">
  /**
   * Пять звёзд. Повторный клик по текущей оценке её снимает — иначе
   * поставленную единицу нельзя было бы убрать.
   */
  let { value, onset }: { value: number; onset: (rating: number) => void } = $props();

  const STARS = [1, 2, 3, 4, 5];
</script>

<span class="rating" class:empty={value === 0}>
  {#each STARS as star (star)}
    <button
      class="star"
      class:on={star <= value}
      title="{star} из 5"
      aria-label="Оценка {star}"
      onclick={(event) => {
        event.stopPropagation();
        onset(value === star ? 0 : star);
      }}
    >
      ★
    </button>
  {/each}
</span>

<style>
  .rating {
    display: inline-flex;
    gap: 1px;
    line-height: 1;
  }
  /* Неоценённый трек не должен кричать пятью серыми звёздами: они
     проявляются по наведению на строку. */
  .rating.empty {
    opacity: 0;
    transition: opacity var(--dur-fast) var(--ease);
  }
  :global(.track:hover) .rating.empty,
  :global(.row:hover) .rating.empty {
    opacity: 1;
  }
  .star {
    padding: 0 1px;
    font-size: 12px;
    line-height: 1;
    color: var(--fg-faint);
    background: none;
    border: none;
    cursor: pointer;
  }
  .star.on {
    color: var(--accent);
  }
  .star:hover {
    color: var(--fg);
  }
</style>
