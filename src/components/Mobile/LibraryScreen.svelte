<script lang="ts">
  import { api, type AlbumRow, type LibraryTrack, type NamedCount, type SmartList } from '../../lib/api';
  import { pushBack } from '../../lib/back';
  import { plural } from '../../lib/format';
  import { pickFolders } from '../../lib/pick';
  import { builtinSmartLists, library } from '../../stores/library.svelte.ts';
  import { settings } from '../../stores/settings.svelte.ts';
  import AlbumGrid from '../Library/AlbumGrid.svelte';
  import ListeningStats from '../Library/ListeningStats.svelte';
  import Icon, { type IconName } from './Icon.svelte';
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
    | { kind: 'tracks'; title: string; tracks: LibraryTrack[]; album: boolean }
    | { kind: 'search'; title: string }
    | { kind: 'stats'; title: string };

  /** Раздел на экране: «назад» закрывает его страницы, только пока он виден. */
  const { shown }: { shown: boolean } = $props();

  let pages = $state<{ page: Page; back: () => void }[]>([]);
  const top = $derived(pages.at(-1));
  let list = $state<ReturnType<typeof LibraryTracks> | null>(null);

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
    open({ kind: 'tracks', title: album.title || 'Без названия', tracks, album: true });
  }

  async function openName(scope: 'artist' | 'genre' | 'folder', item: NamedCount) {
    if (scope === 'artist') {
      // У исполнителя сначала его альбомы, а не свалка всех треков.
      open({ kind: 'albums', title: item.name, albums: await api.libraryAlbums(item.name) });
      return;
    }
    const title = scope === 'folder' ? (item.name.split('/').filter(Boolean).at(-1) ?? item.name) : item.name;
    open({ kind: 'tracks', title, tracks: await api.libraryTracks(scope, item.name), album: false });
  }

  async function openSmart(smart: SmartList) {
    const tracks = await api.librarySmartTracks(smart.rules);
    open({ kind: 'tracks', title: smart.name, tracks, album: false });
  }

  async function playAlbum(album: AlbumRow) {
    const tracks = await api.libraryTracks('album', album.key);
    await api.playlistPlayFiles(
      tracks.map((track) => track.path),
      0,
    );
  }

  const smartLists = $derived([...builtinSmartLists(), ...(settings.current?.smartLists ?? [])]);

  // Недавние альбомы перечитываем после каждого скана: он мог принести новые.
  let recent = $state<AlbumRow[]>([]);
  $effect(() => {
    void library.lastScan;
    void library.stats?.tracks;
    void api
      .libraryRecentAlbums(12)
      .then((albums) => (recent = albums))
      .catch((err) => console.warn('[ringloft] недавние альбомы не пришли', err));
  });

  $effect(() => {
    for (const album of recent) void library.requestCover(album.key);
  });

  async function addFolder() {
    const picked = await pickFolders({ title: 'Папка библиотеки' });
    if (picked.length > 0) await library.addFolders(picked);
  }

  const stats = $derived(library.stats);
  const summary = $derived(
    stats && stats.tracks > 0
      ? `${stats.tracks} ${plural(stats.tracks, 'трек', 'трека', 'треков')} · ${hours(stats.totalDurationMs)}`
      : '',
  );

  /** Длительность всей библиотеки: счёт идёт на часы, секунды тут шум. */
  function hours(ms: number): string {
    const total = Math.round(ms / 3_600_000);
    return total > 0 ? `${total} ${plural(total, 'час', 'часа', 'часов')} музыки` : 'меньше часа музыки';
  }
  const SCAN_PHASES: Record<string, string> = {
    walking: 'обход папок',
    reading: 'чтение тегов',
    cleaning: 'очистка',
  };
</script>

<div class="screen">
  {#if !top}
    <header>
      <h1>Библиотека</h1>
      <button class="key" aria-label="Поиск" onclick={() => open({ kind: 'search', title: 'Поиск' })}>
        <Icon name="search" />
      </button>
      <button class="key" aria-label="Пересканировать" disabled={library.scanning} onclick={() => library.scan()}>
        <Icon name="refresh" />
      </button>
    </header>

    <div class="home">
      {#if library.scanning}
        <p class="status">
          Сканирую{library.progress ? `: ${SCAN_PHASES[library.progress.phase] ?? ''}` : ''}…
        </p>
      {/if}

      {#if library.folders.length === 0}
        <div class="empty">
          <p>Покажите плееру, где лежит музыка: он прочитает теги и разложит её по альбомам, исполнителям и жанрам.</p>
          <button class="primary" onclick={addFolder}>Добавить папку</button>
        </div>
      {:else}
        <div class="tiles">
          {#each SECTIONS as section (section.id)}
            <button class="tile" onclick={() => section.open()}>
              <Icon name={section.icon} />
              <span>{section.label}</span>
            </button>
          {/each}
        </div>

        {#if recent.length > 0}
          <h2>Недавно добавленные</h2>
          <div class="strip">
            {#each recent as album (album.key)}
              <button class="recent" onclick={() => openAlbum(album)}>
                <span class="art">
                  {#if library.covers.get(album.key)}
                    <img src={library.covers.get(album.key)} alt="" loading="lazy" />
                  {:else}
                    <Icon name="note" size={28} />
                  {/if}
                </span>
                <span class="name">{album.title || 'Без названия'}</span>
                <span class="sub">{album.artist ?? '—'}</span>
              </button>
            {/each}
          </div>
        {/if}

        {#if summary}<p class="status">{summary}</p>{/if}
      {/if}
    </div>
  {:else}
    {@const page = top.page}
    <header>
      <button class="key" aria-label="Назад" onclick={top.back}><Icon name="back" /></button>
      <h1>{page.title}</h1>
      {#if page.kind === 'tracks' && page.tracks.length > 0}
        <button class="key accent" aria-label="Играть всё" onclick={() => list?.play(0)}>
          <Icon name="play" />
        </button>
      {/if}
    </header>

    <div class="page">
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
        <LibraryTracks bind:this={list} tracks={page.tracks} showArtist={!page.album} />
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
        <!-- svelte-ignore a11y_autofocus -->
        <input
          class="input search"
          type="search"
          placeholder="Название, исполнитель, альбом"
          autofocus
          value={library.query}
          oninput={(event) => library.search(event.currentTarget.value)}
        />
        <LibraryTracks
          tracks={library.results}
          empty={library.query.trim() ? 'Ничего не нашлось.' : 'Начните печатать — поиск идёт по всей библиотеке.'}
        />
      {:else}
        <div class="scroll">
          <ListeningStats onplay={(path) => void api.playlistPlayFiles([path], 0)} />
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .screen {
    height: 100%;
    display: flex;
    flex-direction: column;
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
  .home,
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  .home {
    padding: 16px;
  }
  .page {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  /* Сетка альбомов с десктопа прокручивается сама — ей нужна вся высота. */
  .page :global(.grid) {
    flex: 1;
    min-height: 0;
  }
  .tiles {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .tile {
    min-height: 60px;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0 14px;
    font: inherit;
    text-align: left;
    color: var(--fg);
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }
  .tile:active {
    background: var(--row-hover);
  }
  .tile :global(svg) {
    color: var(--accent);
  }
  h2 {
    margin: 24px 0 10px;
    font-size: var(--fs-md);
    font-weight: 600;
    color: var(--fg-dim);
  }
  /* Лента едет вбок и упирается в края экрана, а не в отступы страницы. */
  .strip {
    display: flex;
    gap: 12px;
    margin: 0 -16px;
    padding: 0 16px 4px;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .recent {
    flex: none;
    width: 124px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 0;
    font: inherit;
    text-align: left;
    color: var(--fg);
    background: none;
    border: none;
  }
  .art {
    width: 124px;
    height: 124px;
    display: grid;
    place-items: center;
    margin-bottom: 4px;
    overflow: hidden;
    border-radius: var(--radius);
    color: var(--fg-faint);
    background: var(--bg-inset);
  }
  .art img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .name,
  .sub {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .name {
    font-size: var(--fs-sm);
    font-weight: 600;
  }
  .sub {
    font-size: var(--fs-xs);
    color: var(--fg-faint);
  }
  .status {
    margin: 20px 0 0;
    font-size: var(--fs-sm);
    color: var(--fg-faint);
  }
  .empty {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 32px 8px;
    color: var(--fg-dim);
    line-height: 1.5;
  }
  .empty p {
    margin: 0;
  }
  .empty .primary {
    min-height: 48px;
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
