<script lang="ts">
  import { flushSync } from 'svelte';
  import { fly } from 'svelte/transition';

  import { api, type AlbumRow, type LibraryTrack, type NamedCount, type SmartList } from '../../lib/api';
  import { pushBack } from '../../lib/back';
  import { hideKeyboardOnEnter } from '../../lib/keyboard.svelte.ts';
  import { builtinSmartLists, library } from '../../stores/library.svelte.ts';
  import { settings } from '../../stores/settings.svelte.ts';
  import AlbumGrid from '../Library/AlbumGrid.svelte';
  import ListeningStats from '../Library/ListeningStats.svelte';
  import AlbumHead from './AlbumHead.svelte';
  import Icon, { type IconName } from './Icon.svelte';
  import LibraryHome from './LibraryHome.svelte';
  import LibraryNames from './LibraryNames.svelte';
  import LibraryTracks from './LibraryTracks.svelte';

  /**
   * Библиотека: с главной — в раздел, из раздела — до треков. Страницы лежат
   * стопкой, и каждая — слой для кнопки «назад» (см. `lib/back.ts`).
   *
   * Данные страницы грузят сами и держат у себя: стор библиотеки помнит
   * один открытый раздел, как нужно десктопу, а здесь их стопка.
   */
  type Page =
    | { kind: 'albums'; title: string; albums: AlbumRow[] }
    | { kind: 'names'; title: string; scope: 'artist' | 'genre' | 'folder'; items: NamedCount[] }
    | { kind: 'smart'; title: string }
    | { kind: 'tracks'; title: string; tracks: LibraryTrack[]; album?: AlbumRow }
    | { kind: 'search'; title: string }
    | { kind: 'stats'; title: string };

  /** Раздел на экране: «назад» закрывает его страницы, только пока он виден. */
  const { shown }: { shown: boolean } = $props();

  let pages = $state<{ page: Page; back: () => void }[]>([]);
  let searchInput = $state<HTMLInputElement | null>(null);

  /** Фокус — в самом обработчике касания, иначе клавиатура не откроется (см. плейлист). */
  function openSearch() {
    open({ kind: 'search', title: 'Поиск' });
    flushSync();
    searchInput?.focus();
  }

  function open(page: Page) {
    const back = pushBack(
      () => (pages = pages.slice(0, -1)),
      () => shown,
    );
    pages = [...pages, { page, back }];
  }

  const SECTIONS: { id: string; label: string; icon: IconName; open: () => Promise<void> | void }[] = [
    {
      id: 'albums',
      label: 'Альбомы',
      icon: 'album',
      open: async () => open({ kind: 'albums', title: 'Альбомы', albums: await api.libraryAlbums() }),
    },
    {
      id: 'artists',
      label: 'Исполнители',
      icon: 'artist',
      open: async () =>
        open({ kind: 'names', title: 'Исполнители', scope: 'artist', items: await api.libraryArtists() }),
    },
    {
      id: 'genres',
      label: 'Жанры',
      icon: 'genre',
      open: async () =>
        open({ kind: 'names', title: 'Жанры', scope: 'genre', items: await api.libraryGenres() }),
    },
    {
      id: 'folders',
      label: 'Папки',
      icon: 'folder',
      open: async () =>
        open({ kind: 'names', title: 'Папки', scope: 'folder', items: await api.libraryFolders() }),
    },
    { id: 'smart', label: 'Умные списки', icon: 'smart', open: () => open({ kind: 'smart', title: 'Умные списки' }) },
    { id: 'stats', label: 'Итоги', icon: 'stats', open: () => open({ kind: 'stats', title: 'Итоги' }) },
  ];

  async function openAlbum(album: AlbumRow) {
    const tracks = await api.libraryTracks('album', album.key);
    // Название крупно стоит в шапке альбома — в полосе сверху оно было бы повтором.
    open({ kind: 'tracks', title: 'Альбом', tracks, album });
  }

  async function openName(scope: 'artist' | 'genre' | 'folder', item: NamedCount) {
    if (scope === 'artist') {
      // У исполнителя сначала его альбомы, а не свалка всех треков.
      open({ kind: 'albums', title: item.name, albums: await api.libraryAlbums(item.name) });
      return;
    }
    const title = scope === 'folder' ? (item.name.split('/').filter(Boolean).at(-1) ?? item.name) : item.name;
    open({ kind: 'tracks', title, tracks: await api.libraryTracks(scope, item.name) });
  }

  async function openSmart(smart: SmartList) {
    const tracks = await api.librarySmartTracks(smart.rules);
    open({ kind: 'tracks', title: smart.name, tracks });
  }

  /** Выборка играет временной очередью — собранные списки не трогаем. */
  function play(paths: string[]) {
    if (paths.length > 0) void api.playlistPlayFiles(paths, 0);
  }

  async function playAlbum(album: AlbumRow) {
    const tracks = await api.libraryTracks('album', album.key);
    play(tracks.map((track) => track.path));
  }

  const smartLists = $derived([...builtinSmartLists(), ...(settings.current?.smartLists ?? [])]);
</script>

<!-- Страницы лежат стопкой и живут, пока открыты: вернулись назад — та же
     прокрутка, что была. Видна только верхняя, новая въезжает справа, а
     закрытая уезжает туда же и открывает ту, что под ней. -->
<div class="screen">
  <div class="view" hidden={pages.length > 0}>
    <header>
      <h1>Библиотека</h1>
      <button class="key" aria-label="Поиск" onclick={openSearch}>
        <Icon name="search" />
      </button>
      <button class="key" aria-label="Пересканировать" disabled={library.scanning} onclick={() => library.scan()}>
        <Icon name="refresh" />
      </button>
    </header>

    <LibraryHome
      sections={SECTIONS}
      onsection={(id) => SECTIONS.find((section) => section.id === id)?.open()}
      onalbum={openAlbum}
    />
  </div>

  {#each pages as entry, depth (entry)}
    {@const page = entry.page}
    <div
      class="view page"
      hidden={depth !== pages.length - 1}
      in:fly={{ x: 48, duration: 220, opacity: 0 }}
      out:fly={{ x: 48, duration: 180, opacity: 0 }}
    >
      <header>
        <button class="key" aria-label="Назад" onclick={entry.back}><Icon name="back" /></button>
        <h1>{page.title}</h1>
        {#if page.kind === 'tracks' && !page.album && page.tracks.length > 0}
          <button class="key accent" aria-label="Играть всё" onclick={() => play(page.tracks.map((track) => track.path))}>
            <Icon name="play" />
          </button>
        {/if}
      </header>

      {#if page.kind === 'albums'}
        <AlbumGrid albums={page.albums} onopen={openAlbum} onplay={playAlbum} />
      {:else if page.kind === 'names'}
        <LibraryNames
          items={page.items}
          icon={page.scope === 'artist' ? 'artist' : page.scope === 'genre' ? 'genre' : 'folder'}
          folders={page.scope === 'folder'}
          onopen={(item) => openName(page.scope, item)}
        />
      {:else if page.kind === 'tracks'}
        {#if page.album}<AlbumHead album={page.album} tracks={page.tracks} onplay={play} />{/if}
        <LibraryTracks tracks={page.tracks} showArtist={!page.album} />
      {:else if page.kind === 'smart'}
        <div class="scroll">
          {#each smartLists as smart (smart.id)}
            <button class="line" onclick={() => openSmart(smart)}>
              <Icon name="smart" />
              <span>{smart.name}</span>
              <Icon name="forward" size={16} />
            </button>
          {/each}
        </div>
      {:else if page.kind === 'search'}
        <input
          bind:this={searchInput}
          class="input search"
          type="search"
          placeholder="Название, исполнитель, альбом"
          value={library.query}
          oninput={(event) => library.search(event.currentTarget.value)}
          onkeydown={hideKeyboardOnEnter}
        />
        <LibraryTracks
          tracks={library.results}
          empty={library.query.trim() ? 'Ничего не нашлось.' : 'Начните печатать — поиск идёт по всей библиотеке.'}
        />
      {:else}
        <div class="scroll">
          <ListeningStats onplay={(path) => play([path])} />
        </div>
      {/if}
    </div>
  {/each}
</div>

<style>
  .screen {
    position: relative;
    height: 100%;
    overflow: hidden;
  }
  .view {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
  .view[hidden] {
    display: none;
  }
  header {
    flex: none;
    height: 56px;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 4px 0 8px;
    border-bottom: 1px solid var(--border);
  }
  h1 {
    flex: 1;
    min-width: 0;
    margin: 0;
    padding: 0 8px;
    font-size: var(--fs-lg);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .key {
    flex: none;
    width: 44px;
    height: 44px;
    display: grid;
    place-items: center;
    color: var(--fg-dim);
    background: none;
    border: none;
    border-radius: 50%;
  }
  .key:active:not(:disabled) {
    background: var(--row-hover);
  }
  .key:disabled {
    opacity: 0.4;
  }
  .key.accent {
    color: var(--accent);
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  /* Сетка альбомов с десктопа прокручивается сама — ей нужна вся высота. */
  .page > :global(.grid) {
    flex: 1;
    min-height: 0;
  }
  .line {
    width: 100%;
    min-height: 56px;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 0 16px;
    font: inherit;
    text-align: left;
    color: var(--fg);
    background: none;
    border: none;
  }
  .line span {
    flex: 1;
  }
  .line:active {
    background: var(--row-hover);
  }
  .line :global(svg) {
    color: var(--fg-faint);
  }
  .search {
    flex: none;
    height: 44px;
    margin: 10px 12px;
    width: auto;
  }
</style>
