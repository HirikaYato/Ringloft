<script lang="ts">
  /** Текста нет: подсказка и кнопка поиска в интернете. */
  import { player } from '../../stores/player.svelte.ts';

  const isStream = $derived(player.track?.path.startsWith('http') ?? false);
  const searching = $derived(player.lyricsSearch === 'busy');

  /** Итог поиска словами. Пусто — ещё не искали. */
  const searchNote = $derived.by(() => {
    switch (player.lyricsSearch) {
      case 'instrumental':
        return 'В базе это инструментал — текста у трека нет.';
      case 'missing':
        return 'В базе lrclib такого трека нет.';
      case 'noTags':
        return 'Искать нечем: в теге нет исполнителя, и имя файла не помогает.';
      default:
        return '';
    }
  });
</script>

<div class="empty">
  <p class="note">Ни в тегах, ни рядом с файлом текста нет.</p>
  {#if isStream}
    <p class="note dim">У радио текста не найти: ищем по тегам файла.</p>
  {:else}
    <button class="ghost" disabled={searching} onclick={() => void player.fetchLyrics()}>
      {searching ? 'Ищу…' : 'Найти в интернете'}
    </button>
    {#if searchNote}<p class="note dim">{searchNote}</p>{/if}
  {/if}
</div>

<style>
  /* Подсказка и кнопка по центру, а не прижаты к краю. */
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 12px 0;
    font-family: var(--font-ui);
  }
  .note {
    margin: 0;
    color: var(--fg-faint);
    font-size: var(--fs-sm);
    text-align: center;
  }
  .note.dim {
    font-size: var(--fs-xs);
  }
</style>
