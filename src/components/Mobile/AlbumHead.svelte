<script lang="ts">
  import type { AlbumRow, LibraryTrack } from '../../lib/api';
  import { plural } from '../../lib/format';
  import { library } from '../../stores/library.svelte.ts';
  import AddToList from './AddToList.svelte';
  import Icon from './Icon.svelte';

  /**
   * Шапка страницы альбома: обложка, кто и когда, и три действия — играть
   * по порядку, вперемешку и положить весь альбом в список.
   */
  const {
    album,
    tracks,
    onplay,
  }: { album: AlbumRow; tracks: LibraryTrack[]; onplay: (paths: string[]) => void } = $props();

  let adding = $state(false);

  const cover = $derived(library.covers.get(album.key));
  const facts = $derived(
    [
      album.year ? String(album.year) : '',
      `${tracks.length} ${plural(tracks.length, 'трек', 'трека', 'треков')}`,
      length(album.durationMs),
    ]
      .filter(Boolean)
      .join(' · '),
  );

  $effect(() => {
    void library.requestCover(album.key);
  });

  function length(ms: number): string {
    const minutes = Math.round(ms / 60_000);
    if (minutes < 60) return `${minutes} мин`;
    return `${Math.floor(minutes / 60)} ч ${minutes % 60} мин`;
  }

  /** Перемешиваем сами: режим перемешивания у пользователя трогать незачем. */
  function shuffled(): string[] {
    const paths = tracks.map((track) => track.path);
    for (let i = paths.length - 1; i > 0; i--) {
      const j = Math.floor(Math.random() * (i + 1));
      [paths[i], paths[j]] = [paths[j] as string, paths[i] as string];
    }
    return paths;
  }
</script>

<div class="head">
  <div class="top">
    <span class="art">
      {#if cover}
        <img src={cover} alt="" />
      {:else}
        <Icon name="album" size={36} />
      {/if}
    </span>
    <div class="info">
      <h2>{album.title || 'Без названия'}</h2>
      {#if album.artist}<p class="artist">{album.artist}</p>{/if}
      <p class="facts">{facts}</p>
    </div>
  </div>
  <div class="actions">
    <button class="primary" onclick={() => onplay(tracks.map((track) => track.path))}>
      <Icon name="play" size={18} /> Играть
    </button>
    <button class="ghost" onclick={() => onplay(shuffled())}>
      <Icon name="shuffle" size={18} /> Вперемешку
    </button>
    <button class="ghost round" aria-label="Добавить в список" onclick={() => (adding = true)}>
      <Icon name="plus" size={20} />
    </button>
  </div>
</div>

{#if adding}
  <AddToList
    paths={tracks.map((track) => track.path)}
    title="Альбом — в список"
    newName={album.title || undefined}
    onclose={() => (adding = false)}
  />
{/if}

<style>
  .head {
    flex: none;
    padding: 16px 16px 12px;
    border-bottom: 1px solid var(--border);
  }
  .top {
    display: flex;
    gap: 16px;
    align-items: center;
  }
  .art {
    flex: none;
    width: 104px;
    height: 104px;
    display: grid;
    place-items: center;
    overflow: hidden;
    border-radius: var(--radius-lg);
    color: var(--fg-faint);
    background: var(--bg-inset);
    box-shadow: var(--shadow-pop);
  }
  .art img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .info {
    min-width: 0;
  }
  h2 {
    margin: 0;
    font-size: var(--fs-lg);
    font-weight: 700;
    line-height: 1.25;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  p {
    margin: 4px 0 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .artist {
    color: var(--fg-dim);
  }
  .facts {
    font-size: var(--fs-sm);
    color: var(--fg-faint);
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: 14px;
  }
  .actions button {
    min-height: 40px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 0 16px;
    border-radius: 20px;
  }
  .actions .primary {
    flex: 1;
  }
  .actions .ghost:not(.round) {
    flex: 1;
  }
  .round {
    flex: none;
    width: 40px;
    padding: 0;
  }
</style>
