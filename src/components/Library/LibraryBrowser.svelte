<script lang="ts">
  import { covers } from '../../stores/covers.svelte.ts';
  import ListeningStats from './ListeningStats.svelte';
  import { metrics } from '../../lib/typography.svelte.ts';
  import AlbumGrid from './AlbumGrid.svelte';
  import ContextMenu, { type MenuItem } from '../Common/ContextMenu.svelte';
  import Rating from './Rating.svelte';
  import TagEditor from '../Playlist/TagEditor.svelte';
  import VirtualList from '../Common/VirtualList.svelte';
  import { formatTime } from '../../lib/format';
  import { api, type AlbumRow } from '../../lib/api';
  import { library } from '../../stores/library.svelte.ts';
  import { playlist } from '../../stores/playlist.svelte.ts';

  const MODE_TITLES = {
    albums: 'Все альбомы',
    artists: 'Исполнители',
    genres: 'Жанры',
    folders: 'Папки',
    smart: 'Умные списки',
    stats: 'Итоги прослушивания',
  } as const;

  const tracks = $derived(library.selection?.scope === 'album' ? library.tracks : library.tracks);
  const showGrid = $derived(library.albums.length > 0 && library.tracks.length === 0);

  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let editing = $state<string[] | null>(null);

  function trackMenu(event: MouseEvent, path: string) {
    event.preventDefault();
    menu = {
      x: event.clientX,
      y: event.clientY,
      items: [
        { label: 'Играть', action: () => void addTracks([path], true) },
        { label: 'Добавить в плейлист', action: () => void addTracks([path], false) },
        { separator: true },
        { label: 'Изменить теги', action: () => (editing = [path]) },
        { label: 'Открыть папку с треком', action: () => void api.revealInFolder(path) },
      ],
    };
  }

  async function addTracks(paths: string[], play: boolean) {
    if (paths.length === 0) return;
    const startIndex = playlist.count;
    const added = await playlist.add(paths);
    if (added > 0 && play) await playlist.play(startIndex);
  }

  function albumMenu(event: MouseEvent, album: AlbumRow) {
    event.preventDefault();
    menu = {
      x: event.clientX,
      y: event.clientY,
      items: [
        { label: 'Играть', action: () => void playAlbum(album) },
        { label: 'Открыть', action: () => void openAlbum(album) },
        { separator: true },
        {
          label: album.hasCover ? 'Заменить обложку из интернета' : 'Найти обложку в интернете',
          disabled: covers.busyAlbum !== null,
          action: () => void covers.fetchAlbum(album.key),
        },
      ],
    };
  }

  async function openAlbum(album: AlbumRow) {
    await library.select('album', album.key, album.title);
  }

  async function playAlbum(album: AlbumRow) {
    const tracks = await library.tracksOf('album', album.key);
    await addTracks(
      tracks.map((track) => track.path),
      true,
    );
  }
</script>

<section class="browser">
  <header>
    <button class="crumb" disabled={!library.selection} onclick={() => library.back()}>
      {MODE_TITLES[library.mode]}
    </button>
    {#if library.selection}
      <span class="sep">›</span>
      <span class="crumb current">{library.selection.label}</span>
    {:else if library.mode === 'smart' && library.smart}
      <span class="sep">›</span>
      <span class="crumb current">{library.smart.name}</span>
    {/if}

    <span class="spacer"></span>

    {#if showGrid && !library.selection}
      <button
        class="ghost"
        disabled={covers.running}
        title="Для альбомов без картинки: iTunes и Cover Art Archive, с записью в файлы"
        onclick={() => void covers.scanMissing()}
      >
        Найти недостающие обложки
      </button>
    {/if}
    {#if tracks.length > 0}
      <button class="ghost" onclick={() => addTracks(tracks.map((t) => t.path), false)}>
        В плейлист
      </button>
      <button class="ghost" onclick={() => addTracks(tracks.map((t) => t.path), true)}>
        Играть
      </button>
    {/if}
  </header>

  {#if library.mode === 'stats'}
    <ListeningStats onplay={(path) => addTracks([path], true)} />
  {:else if showGrid}
    <AlbumGrid albums={library.albums} onopen={openAlbum} onplay={playAlbum} oncontext={albumMenu} />
    <p class="hint">Клик по альбому — открыть список треков, двойной — включить целиком.</p>
  {:else if tracks.length > 0}
    <VirtualList items={tracks} rowHeight={metrics.row}>
      {#snippet row(track, index)}
        <button
          class="track"
          ondblclick={() => addTracks([track.path], true)}
          oncontextmenu={(event) => trackMenu(event, track.path)}
        >
          <span class="num">{track.trackNo ?? index + 1}</span>
          <span class="name">{track.title}</span>
          <span class="artist">{track.artist ?? ''}</span>
          <span class="plays" title="Прослушиваний">
            {track.playCount > 0 ? track.playCount : ''}
          </span>
          <Rating value={track.rating} onset={(rating) => library.setRating(track.path, rating)} />
          <span class="time">{track.durationMs ? formatTime(track.durationMs) : ''}</span>
        </button>
      {/snippet}
    </VirtualList>
  {:else}
    <p class="empty">
      {#if library.mode === 'albums'}
        Альбомов пока нет — добавь папку с музыкой слева.
      {:else}
        Выбери что-нибудь в списке слева.
      {/if}
    </p>
  {/if}
</section>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

{#if editing}
  <TagEditor paths={editing} onclose={() => (editing = null)} />
{/if}

<style>
  /* Внутри общей карточки с колонкой — своей рамки нет. Шапка по высоте
     как у колонки: линии сходятся. */
  .browser {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg-elev);
  }
  header {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: var(--chrome-h);
    box-sizing: border-box;
    padding: 0 10px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-inset);
    flex: none;
  }
  .crumb {
    font: inherit;
    font-size: var(--fs-sm);
    background: none;
    border: 0;
    color: var(--fg-dim);
    cursor: pointer;
    padding: 0;
  }
  .crumb:disabled {
    cursor: default;
  }
  .crumb.current {
    color: var(--fg);
    font-weight: 600;
  }
  .sep {
    color: var(--fg-faint);
  }
  .spacer {
    flex: 1;
  }
  .ghost {
    font-size: var(--fs-xs);
    background: var(--bg-elev);
    padding: 3px 10px;
  }
  .ghost:hover {
    background: var(--row-hover);
    color: var(--fg);
  }
  .track {
    width: 100%;
    height: 100%;
    display: grid;
    grid-template-columns: calc(var(--fs-md) * 2.7) minmax(0, 1fr) minmax(0, 1fr) 28px calc(var(--fs-md) * 4.8) var(--col-time);
    align-items: center;
    gap: 8px;
    font: inherit;
    font-size: var(--fs-sm);
    background: none;
    border: 0;
    color: var(--fg);
    text-align: left;
    padding: 4px 10px;
    cursor: default;
  }
  .track:nth-child(even) {
    background: var(--row-alt);
  }
  .track:hover {
    background: var(--row-hover);
  }
  .num,
  .plays {
    color: var(--fg-faint);
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    text-align: right;
  }
  .time {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--fg-faint);
    text-align: right;
  }
  .name,
  .artist {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .artist {
    color: var(--fg-dim);
  }
  .empty,
  .hint {
    margin: 0;
    padding: 10px 12px;
    font-size: var(--fs-xs);
    color: var(--fg-faint);
  }
  .empty {
    padding: 24px;
    text-align: center;
    font-size: var(--fs-sm);
  }
</style>
