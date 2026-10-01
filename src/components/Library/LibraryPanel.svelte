<script lang="ts">
  import { metrics } from '../../lib/typography.svelte.ts';
  import { open } from '@tauri-apps/plugin-dialog';

  import Maintenance from './Maintenance.svelte';
  import SmartEditor from './SmartEditor.svelte';
  import VirtualList from '../Common/VirtualList.svelte';
  import ContextMenu, { type MenuItem } from '../Common/ContextMenu.svelte';
  import { covers } from '../../stores/covers.svelte.ts';

  import { formatTime, plural } from '../../lib/format';
  import type { SmartList } from '../../lib/api';
  import type { BrowseMode } from '../../stores/library.svelte.ts';
  import { builtinSmartLists, library } from '../../stores/library.svelte.ts';
  import { lyricsScan } from '../../stores/lyrics.svelte.ts';
  import { playlist } from '../../stores/playlist.svelte.ts';
  import { settings } from '../../stores/settings.svelte.ts';

  const MODES: { id: BrowseMode; label: string; icon: string }[] = [
    { id: 'albums', label: 'Альбомы', icon: 'M2.5 2.5h11v11h-11zM8 5.6a2.4 2.4 0 1 0 0 4.8 2.4 2.4 0 0 0 0-4.8' },
    { id: 'artists', label: 'Исполнители', icon: 'M8 7.6a2.6 2.6 0 1 0 0-5.2 2.6 2.6 0 0 0 0 5.2M3 13.6c.6-2.6 2.6-4 5-4s4.4 1.4 5 4' },
    { id: 'genres', label: 'Жанры', icon: 'M2.5 8.2V2.5h5.7l5.3 5.3-5.7 5.7zM5.4 5.4h.01' },
    { id: 'folders', label: 'Папки', icon: 'M2 4.5V12a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1V6a1 1 0 0 0-1-1H8L6.5 3.5H3a1 1 0 0 0-1 1' },
    { id: 'smart', label: 'Умные списки', icon: 'M8 2v2.5M8 11.5V14M2 8h2.5M11.5 8H14M4 4l1.6 1.6M10.4 10.4 12 12M12 4l-1.6 1.6M5.6 10.4 4 12' },
    { id: 'stats', label: 'Итоги', icon: 'M2.5 13.5h11M4.5 11V8M7.5 11V4.5M10.5 11V6.5M13 11V9' },
  ];

  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  /** Редкое — в меню: четыре кнопки в два ряда занимали пол-колонки. */
  function moreMenu(event: MouseEvent) {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    menu = {
      x: rect.left,
      y: rect.bottom + 4,
      items: [
        { label: 'Проверить коллекцию…', action: () => (checking = true) },
        lyricsScan.running
          ? { label: 'Остановить поиск текстов', action: () => lyricsScan.cancel() }
          : { label: 'Найти тексты для всей библиотеки', action: () => void lyricsScan.scanLibrary() },
        covers.running
          ? { label: 'Остановить поиск обложек', action: () => covers.cancel() }
          : { label: 'Найти недостающие обложки', action: () => void covers.scanMissing() },
        { separator: true },
        { label: 'Добавить папку…', action: () => void addFolder() },
      ],
    };
  }

  /** У умных списков области обзора нет: они сами себе выборка. */
  const SCOPE_OF: Record<Exclude<BrowseMode, 'smart' | 'stats'>, 'artist' | 'genre' | 'folder'> = {
    albums: 'artist',
    artists: 'artist',
    genres: 'genre',
    folders: 'folder',
  };

  function scopeOf(mode: BrowseMode): 'artist' | 'genre' | 'folder' {
    return mode === 'smart' || mode === 'stats' ? 'artist' : SCOPE_OF[mode];
  }

  /** Встроенные списки плюс свои: правятся только свои. */
  const smartLists = $derived([...builtinSmartLists(), ...(settings.current?.smartLists ?? [])]);
  let editing = $state<SmartList | null>(null);
  /** Открыта проверка коллекции. */
  let checking = $state(false);

  /** Ноль — «списка ещё нет»: настоящий номер выдаётся при сохранении. */
  function emptyList(): SmartList {
    return {
      id: 0,
      name: '',
      rules: { rules: [], sort: 'added', descending: true, limit: null },
    };
  }

  function editList(event: MouseEvent, list: SmartList) {
    event.preventDefault();
    // Встроенные не правим: они одинаковые для всех.
    if (list.id > 0) editing = list;
  }

  function saveList(list: SmartList) {
    const saved = { ...list, id: list.id > 0 ? list.id : Date.now() };
    const own = (settings.current?.smartLists ?? []).filter((item) => item.id !== saved.id);
    void settings.patch({ smartLists: [...own, saved] });
    void library.loadSmart(saved);
  }

  function deleteList(id: number) {
    const own = (settings.current?.smartLists ?? []).filter((item) => item.id !== id);
    void settings.patch({ smartLists: own });
  }

  const stats = $derived(library.stats);
  const progress = $derived(library.progress);
  const percent = $derived(
    progress && progress.total > 0 ? (progress.processed / progress.total) * 100 : 0,
  );

  const PHASES: Record<string, string> = {
    walking: 'обход папок',
    reading: 'чтение тегов',
    cleaning: 'очистка',
    cancelled: 'отменено',
  };

  async function addFolder() {
    const selected = await open({ multiple: true, directory: true });
    if (Array.isArray(selected)) await library.addFolders(selected);
    else if (typeof selected === 'string') await library.addFolders([selected]);
  }

  /** Двойной клик по находке — добавить в плейлист и включить. */
  async function playTrack(path: string) {
    const index = playlist.count;
    const added = await playlist.add([path]);
    if (added > 0) await playlist.play(index);
  }

  function shortFolder(path: string): string {
    const parts = path.split('/').filter(Boolean);
    return parts.length > 2 ? `…/${parts.slice(-2).join('/')}` : path;
  }
</script>

<aside class="library">
  <header>
    <span class="title">Библиотека</span>
    <button class="icon" title="Добавить папку" aria-label="Добавить папку" onclick={addFolder}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2 4.5V12a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1V6a1 1 0 0 0-1-1H8L6.5 3.5H3a1 1 0 0 0-1 1M8 7v4M6 9h4" /></svg>
    </button>
    {#if library.scanning}
      <button class="icon" title="Остановить сканирование" aria-label="Остановить сканирование" onclick={() => library.cancelScan()}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="4" y="4" width="8" height="8" rx="1.5" /></svg>
      </button>
    {:else}
      <button
        class="icon"
        title="Пересканировать папки"
        aria-label="Пересканировать папки"
        disabled={library.folders.length === 0}
        onclick={() => library.scan()}
      >
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M13.2 8a5.2 5.2 0 1 1-1.5-3.7M13.2 2.4v2.9h-2.9" /></svg>
      </button>
    {/if}
    <button class="icon" title="Ещё" aria-label="Ещё" onclick={moreMenu}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 8h.01M8 8h.01M12.5 8h.01" /></svg>
    </button>
  </header>

  <div class="summary">
    {#if stats}
      <span class="counts">
        {[
          `${stats.tracks} ${plural(stats.tracks, 'трек', 'трека', 'треков')}`,
          `${stats.albums} ${plural(stats.albums, 'альбом', 'альбома', 'альбомов')}`,
          `${stats.artists} ${plural(stats.artists, 'исполнитель', 'исполнителя', 'исполнителей')}`,
          stats.totalDurationMs > 0 ? formatTime(stats.totalDurationMs) : '',
        ]
          .filter(Boolean)
          .join(' · ')}
      </span>
    {/if}
    {#each library.folders as folder (folder)}
      <div class="folder" title={folder}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2 4.5V12a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1V6a1 1 0 0 0-1-1H8L6.5 3.5H3a1 1 0 0 0-1 1" /></svg>
        <span>{shortFolder(folder)}</span>
        <button class="x" title="Убрать папку из библиотеки" onclick={() => library.removeFolder(folder)}>×</button>
      </div>
    {:else}
      <button class="link" onclick={addFolder}>Добавить папку с музыкой</button>
    {/each}
    {#if progress}
      <div class="scan">
        <div class="bar"><div class="fill" style:width="{percent}%"></div></div>
        <span class="counts">
          {PHASES[progress.phase] ?? progress.phase}
          {#if progress.total > 0}
            — {progress.processed} из {progress.total}
          {:else if progress.processed > 0}
            — найдено {progress.processed}
          {/if}
        </span>
      </div>
    {:else if library.lastScan}
      <span class="counts dim">
        Последний скан: +{library.lastScan.added}, обновлено {library.lastScan.updated}, удалено
        {library.lastScan.removed}
      </span>
    {/if}
  </div>

  <nav class="modes" aria-label="Разделы библиотеки">
    {#each MODES as mode (mode.id)}
      <button
        class="mode"
        class:active={library.mode === mode.id}
        onclick={() => library.loadMode(mode.id)}
      >
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d={mode.icon} /></svg>
        <span>{mode.label}</span>
      </button>
    {/each}
  </nav>

  <div class="search-box">
    <input
      class="input search"
      type="search"
      placeholder="Поиск по библиотеке"
      value={library.query}
      oninput={(event) => library.search(event.currentTarget.value)}
    />
  </div>

  {#if library.query}
    <div class="results">
      {#each library.results as track (track.id)}
        <button class="result" ondblclick={() => playTrack(track.path)} title={track.path}>
          <span class="title">{track.title}</span>
          <span class="sub">{track.artist ?? '—'}{track.album ? ` · ${track.album}` : ''}</span>
          {#if track.durationMs}<span class="time">{formatTime(track.durationMs)}</span>{/if}
        </button>
      {:else}
        {#if !library.searching}<p class="hint">Ничего не нашлось.</p>{/if}
      {/each}
    </div>
  {:else if library.mode === 'smart'}
    <div class="names plain">
      {#each smartLists as list (list.id)}
        <button
          class="name"
          class:active={library.smart?.id === list.id}
          title={list.id < 0 ? 'Встроенный список' : 'Свой список: правый клик — правка'}
          onclick={() => library.loadSmart(list)}
          oncontextmenu={(event) => editList(event, list)}
        >
          <span>{list.name}</span>
          {#if list.id > 0}<span class="count">свой</span>{/if}
        </button>
      {/each}
      <button class="name add" onclick={() => (editing = emptyList())}>+ свой список</button>
    </div>
  {:else if library.names.length > 0}
    <div class="names">
      <VirtualList items={library.names} rowHeight={metrics.compact}>
        {#snippet row(item)}
          <button
            class="name"
            class:active={library.selection?.value === item.name}
            title={item.name}
            onclick={() => library.select(scopeOf(library.mode), item.name, shortFolder(item.name))}
          >
            <span>{library.mode === 'folders' ? shortFolder(item.name) : item.name}</span>
            <span class="count">{item.tracks}</span>
          </button>
        {/snippet}
      </VirtualList>
    </div>
  {:else}
    <div class="names"></div>
  {/if}
</aside>

{#if menu}
  <ContextMenu items={menu.items} x={menu.x} y={menu.y} onclose={() => (menu = null)} />
{/if}

{#if checking}
  <Maintenance onclose={() => (checking = false)} />
{/if}

{#if editing}
  <SmartEditor
    list={editing}
    onsave={saveList}
    ondelete={deleteList}
    onclose={() => (editing = null)}
  />
{/if}

<style>
  /* Колонка в общей карточке с обзором — как колонка плейлистов: шапка в
     линию с полосой справа, действия значками, разделы списком. */
  .library {
    width: clamp(220px, calc(var(--fs-md) * 17), 340px);
    flex: none;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg-inset);
    border-right: 1px solid var(--border);
  }
  header {
    display: flex;
    align-items: center;
    gap: 2px;
    min-height: var(--chrome-h);
    padding: 0 6px 0 14px;
    border-bottom: 1px solid var(--border);
    box-sizing: border-box;
  }
  .title {
    flex: 1;
    font-size: var(--fs-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--fg-faint);
  }
  .icon {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    padding: 0;
    color: var(--fg-faint);
    background: none;
    border: 0;
    border-radius: var(--radius);
    cursor: pointer;
  }
  .icon:hover:not(:disabled) {
    color: var(--fg);
    background: var(--row-hover);
  }
  .icon:disabled {
    opacity: 0.4;
    cursor: default;
  }
  svg {
    width: 16px;
    height: 16px;
    flex: none;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.4;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .summary {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
  }
  .counts {
    font-size: var(--fs-xs);
    color: var(--fg-dim);
  }
  .counts.dim {
    color: var(--fg-faint);
  }
  .folder {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-xs);
    color: var(--fg-dim);
  }
  .folder svg {
    width: 13px;
    height: 13px;
    color: var(--fg-faint);
  }
  .folder span {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .x {
    padding: 0 4px;
    font-size: var(--fs-sm);
    color: var(--fg-faint);
    background: none;
    border: 0;
    cursor: pointer;
    opacity: 0;
  }
  .folder:hover .x {
    opacity: 1;
  }
  .x:hover {
    color: var(--danger);
  }
  .link {
    align-self: flex-start;
    padding: 0;
    font: inherit;
    font-size: var(--fs-sm);
    color: var(--accent);
    background: none;
    border: 0;
    cursor: pointer;
  }
  .scan .bar {
    height: 3px;
    margin-bottom: 4px;
    border-radius: 2px;
    background: var(--bg-elev);
    overflow: hidden;
  }
  .scan .fill {
    height: 100%;
    background: var(--accent);
    transition: width 120ms linear;
  }
  .modes {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px;
    border-bottom: 1px solid var(--border);
  }
  .mode {
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: var(--row-h);
    padding: 0 10px;
    font: inherit;
    font-size: var(--fs-sm);
    text-align: left;
    color: var(--fg-dim);
    background: none;
    border: 0;
    border-radius: 8px;
    cursor: pointer;
  }
  .mode svg {
    color: var(--fg-faint);
  }
  .mode:hover {
    color: var(--fg);
    background: var(--row-hover);
  }
  .mode.active {
    color: var(--fg);
    background: var(--accent-soft);
  }
  .mode.active svg {
    color: var(--accent);
  }
  .mode.active::before {
    content: '';
    position: absolute;
    left: 0;
    top: 22%;
    bottom: 22%;
    width: 3px;
    border-radius: 2px;
    background: var(--accent);
  }
  .search-box {
    padding: 8px;
  }
  .search {
    border-radius: 999px;
    padding-left: 12px;
  }
  .search::-webkit-search-cancel-button {
    display: none;
  }
  .names {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: 0 8px 8px;
  }
  .names.plain {
    overflow-y: auto;
    gap: 2px;
  }
  .name {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 100%;
    min-height: var(--row-h-compact);
    padding: 0 10px;
    font: inherit;
    font-size: var(--fs-sm);
    text-align: left;
    color: var(--fg-dim);
    background: none;
    border: 0;
    border-radius: 6px;
    cursor: pointer;
  }
  .name span:first-child {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .name:hover {
    color: var(--fg);
    background: var(--row-hover);
  }
  .name.active {
    color: var(--fg);
    background: var(--accent-soft);
  }
  .name.add {
    color: var(--accent);
  }
  .count {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--fg-faint);
  }
  .results {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 0 8px 8px;
  }
  .result {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 0 8px;
    padding: 6px 10px;
    font: inherit;
    text-align: left;
    color: var(--fg);
    background: none;
    border: 0;
    border-radius: 6px;
    cursor: pointer;
  }
  .result:hover {
    background: var(--row-hover);
  }
  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--fs-sm);
  }
  .result .title {
    text-transform: none;
    letter-spacing: 0;
    font-weight: 400;
    color: var(--fg);
  }
  .sub {
    grid-column: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--fs-xs);
    color: var(--fg-dim);
  }
  .time {
    grid-column: 2;
    grid-row: 1 / span 2;
    align-self: center;
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--fg-faint);
  }
  .hint {
    margin: 4px 10px;
    font-size: var(--fs-xs);
    color: var(--fg-faint);
  }
</style>
