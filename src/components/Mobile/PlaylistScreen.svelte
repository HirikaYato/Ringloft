<script lang="ts">
  import { api, type PlaylistRow } from '../../lib/api';
  import { forgetBack, pushBack } from '../../lib/back';
  import { plural } from '../../lib/format';
  import { pickFiles, pickFolders } from '../../lib/pick';
  import { player } from '../../stores/player.svelte.ts';
  import { playlist } from '../../stores/playlist.svelte.ts';
  import Prompt from '../Common/Prompt.svelte';
  import Icon from './Icon.svelte';
  import Sheet from './Sheet.svelte';
  import SheetItem from './SheetItem.svelte';
  import TrackRow from './TrackRow.svelte';

  /**
   * Открытый плейлист. Список виртуальный, как на десктопе: фронт держит
   * только окно строк вокруг экрана, остальное подгружается при прокрутке.
   *
   * Выделение — по номерам строк, а не по индексам: в поиске показана другая
   * выборка, и индекс там значил бы не то.
   */
  const ROW = 60;
  const PAD = 8;
  const AUDIO = [
    'mp3', 'mp2', 'flac', 'wav', 'wave', 'ogg', 'oga', 'opus', 'weba', 'm4a', 'm4b', 'mp4', 'aac',
    'mka', 'aiff', 'aif',
  ];

  /** Раздел на экране: «назад» выходит из поиска и выделения, только пока он виден. */
  const { shown }: { shown: boolean } = $props();

  let viewport = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let height = $state(0);
  let searching = $state(false);
  let picked = $state<Set<number>>(new Set());
  let sheet = $state<'none' | 'lists' | 'add' | 'more'>('none');
  let prompt = $state<'none' | 'stream' | 'rename'>('none');

  const active = $derived(playlist.tabs.find((tab) => tab.isActive));
  const query = $derived(playlist.query.trim());
  const total = $derived(query ? playlist.found.length : playlist.count);
  const first = $derived(Math.max(0, Math.floor(scrollTop / ROW) - PAD));
  const last = $derived(Math.min(total, Math.ceil((scrollTop + height) / ROW) + PAD));
  const visible = $derived(Array.from({ length: Math.max(0, last - first) }, (_, nth) => first + nth));

  function rowAt(index: number): PlaylistRow | undefined {
    return query ? playlist.found[index] : playlist.row(index);
  }

  $effect(() => {
    if (!query) void playlist.ensureRange(first, last);
  });

  // Другой список или другая выборка — начинаем сверху и без выделения:
  // прежнее смещение показывало бы середину чужого списка. Следим за номером
  // вкладки, а не за ней самой: вкладки приезжают заново на каждое изменение
  // списка, и по объекту прокрутка сбрасывалась бы при каждом обновлении.
  const activeId = $derived(active?.id);
  $effect(() => {
    void activeId;
    void query;
    picked = new Set();
    if (viewport) viewport.scrollTop = 0;
    scrollTop = 0;
  });

  // Поиск и выделение — режимы, из которых выходят кнопкой «назад».
  $effect(() => {
    if (!searching) return;
    const leave = () => stopSearch();
    pushBack(leave, () => shown);
    return () => void forgetBack(leave);
  });

  const picking = $derived(picked.size > 0);
  $effect(() => {
    if (!picking) return;
    const leave = () => (picked = new Set());
    pushBack(leave, () => shown);
    return () => void forgetBack(leave);
  });

  function press(row: PlaylistRow) {
    if (picked.size > 0) toggle(row);
    else void playlist.play(row.index);
  }

  function toggle(row: PlaylistRow) {
    const next = new Set(picked);
    if (!next.delete(row.id)) next.add(row.id);
    picked = next;
  }

  async function removePicked() {
    const ids = [...picked];
    picked = new Set();
    await api.playlistRemove(ids);
  }

  async function enqueuePicked() {
    const ids = [...picked];
    picked = new Set();
    await api.playlistEnqueue(ids);
  }

  async function add(paths: string[]) {
    if (paths.length === 0) return;
    const startIndex = playlist.count;
    const added = await playlist.add(paths);
    if (added > 0 && player.status === 'idle') await playlist.play(startIndex);
  }

  function stopSearch() {
    searching = false;
    playlist.setQuery('');
  }

  /** Адрес проверяем до добавления: иначе ошибка всплывёт только при игре. */
  function checkStreamUrl(value: string): string | null {
    let url: URL;
    try {
      url = new URL(value);
    } catch {
      return 'Это не похоже на адрес. Нужен полный, вида https://…';
    }
    return url.protocol === 'http:' || url.protocol === 'https:'
      ? null
      : 'Плеер умеет только http и https.';
  }
</script>

<div class="screen">
  {#if picked.size > 0}
    <header>
      <button class="key" aria-label="Снять выделение" onclick={() => (picked = new Set())}>
        <Icon name="close" />
      </button>
      <span class="count">Выбрано: {picked.size}</span>
      <button class="key" aria-label="В очередь" onclick={enqueuePicked}><Icon name="queue" /></button>
      <button class="key" aria-label="Убрать из списка" onclick={removePicked}><Icon name="trash" /></button>
    </header>
  {:else if searching}
    <header>
      <button class="key" aria-label="Закрыть поиск" onclick={stopSearch}><Icon name="back" /></button>
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="input search"
        type="search"
        placeholder="Название, исполнитель, альбом"
        autofocus
        value={playlist.query}
        oninput={(event) => playlist.setQuery(event.currentTarget.value)}
      />
    </header>
  {:else}
    <header>
      <button class="name" onclick={() => (sheet = 'lists')}>
        <span>{active?.name ?? 'Плейлист'}</span>
        <Icon name="down" size={18} />
      </button>
      <button class="key" aria-label="Поиск" onclick={() => (searching = true)}><Icon name="search" /></button>
      <button class="key" aria-label="Добавить" onclick={() => (sheet = 'add')}><Icon name="plus" /></button>
      <button class="key" aria-label="Действия со списком" onclick={() => (sheet = 'more')}><Icon name="more" /></button>
    </header>
  {/if}

  <div
    class="viewport"
    bind:this={viewport}
    bind:clientHeight={height}
    onscroll={(event) => (scrollTop = event.currentTarget.scrollTop)}
  >
    {#if total === 0}
      <div class="empty">
        {#if query}
          <p>Ничего не нашлось.</p>
        {:else}
          <p>В этом списке пока пусто.</p>
          <button class="primary" onclick={() => (sheet = 'add')}>Добавить музыку</button>
        {/if}
      </div>
    {:else}
      <div class="spacer" style:height="{total * ROW}px">
        {#each visible as index (index)}
          {@const row = rowAt(index)}
          <div class="slot" style:transform="translateY({index * ROW}px)">
            {#if row}
              <TrackRow
                path={row.path}
                title={row.title}
                subtitle={[row.artist, row.album].filter(Boolean).join(' — ')}
                durationMs={row.durationMs}
                current={row.isCurrent}
                selected={picked.has(row.id)}
                onpress={() => press(row)}
                onhold={() => toggle(row)}
              />
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

{#if sheet === 'lists'}
  <Sheet title="Плейлисты" onclose={() => (sheet = 'none')}>
    {#snippet children(close)}
      {#each playlist.tabs as tab (tab.id)}
        <SheetItem
          icon={tab.isTemporary ? 'note' : 'playlist'}
          label={tab.name}
          note={tab.isPlaying ? `играет · ${tab.count}` : String(tab.count)}
          active={tab.isActive}
          onclick={() => {
            void playlist.activateTab(tab.id);
            close();
          }}
        />
      {/each}
      <SheetItem
        icon="plus"
        label="Новый список"
        onclick={() => {
          void playlist.createTab();
          close();
        }}
      />
    {/snippet}
  </Sheet>
{/if}

{#if sheet === 'add'}
  <Sheet title="Добавить в список" onclose={() => (sheet = 'none')}>
    {#snippet children(close)}
      <SheetItem
        icon="folder"
        label="Папку"
        onclick={() => close(async () => add(await pickFolders({ title: 'Добавить папку' })))}
      />
      <SheetItem
        icon="note"
        label="Файлы"
        onclick={() =>
          close(async () =>
            add(
              await pickFiles({
                title: 'Добавить файлы',
                filter: { name: 'Аудио', extensions: AUDIO },
                multiple: true,
              }),
            ),
          )}
      />
      <SheetItem icon="radio" label="Интернет-радио" onclick={() => close(() => (prompt = 'stream'))} />
      <SheetItem
        icon="playlist"
        label="Открыть список (m3u, cue)"
        onclick={() =>
          close(async () => {
            const [list] = await pickFiles({
              title: 'Открыть список',
              filter: { name: 'Списки', extensions: ['m3u', 'm3u8', 'cue'] },
            });
            if (list) await playlist.importList(list);
          })}
      />
    {/snippet}
  </Sheet>
{/if}

{#if sheet === 'more' && active}
  {@const tab = active}
  <Sheet
    title="{tab.name} — {tab.count} {plural(tab.count, 'трек', 'трека', 'треков')}"
    onclose={() => (sheet = 'none')}
  >
    {#snippet children(close)}
      <SheetItem icon="edit" label="Переименовать" onclick={() => close(() => (prompt = 'rename'))} />
      <SheetItem
        icon="folder"
        label="Папки списка"
        note={tab.sources > 0 ? String(tab.sources) : ''}
        onclick={() => close(() => (playlist.sourcesFor = tab.id))}
      />
      {#if tab.sources > 0}
        <SheetItem
          icon="refresh"
          label="Обновить по папкам"
          onclick={() => {
            void playlist.refreshSources(tab.id);
            close();
          }}
        />
      {/if}
      <SheetItem
        icon="trash"
        label="Убрать пропавшие файлы"
        onclick={() => {
          void playlist.dropMissing();
          close();
        }}
      />
      <SheetItem
        icon="close"
        label="Очистить список"
        danger
        disabled={tab.count === 0}
        onclick={() => {
          void playlist.clear();
          close();
        }}
      />
      <SheetItem
        icon="close"
        label="Закрыть список"
        danger
        disabled={playlist.tabs.length < 2}
        onclick={() => close(() => playlist.requestClose(tab.id))}
      />
    {/snippet}
  </Sheet>
{/if}

{#if prompt === 'stream'}
  <Prompt
    title="Интернет-радио"
    label="Адрес потока"
    placeholder="https://example.org/stream"
    hint="Поток добавится строкой в список. Перемотки у радио нет, название приходит из эфира."
    validate={checkStreamUrl}
    onsubmit={(url) => void add([url])}
    onclose={() => (prompt = 'none')}
  />
{/if}

{#if prompt === 'rename' && active}
  {@const tab = active}
  <Prompt
    title="Переименовать список"
    label="Название"
    placeholder={tab.name}
    confirm="Переименовать"
    onsubmit={(name) => void playlist.renameTab(tab.id, name)}
    onclose={() => (prompt = 'none')}
  />
{/if}

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
  .name {
    flex: 1;
    min-width: 0;
    height: 44px;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 8px;
    font: inherit;
    font-size: var(--fs-lg);
    font-weight: 600;
    color: var(--fg);
    background: none;
    border: none;
  }
  .name span {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .count {
    flex: 1;
    padding: 0 8px;
    font-weight: 600;
  }
  .search {
    flex: 1;
    min-width: 0;
    height: 40px;
    margin-right: 8px;
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
  .key:active {
    background: var(--row-hover);
  }
  .viewport {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  .spacer {
    position: relative;
  }
  .slot {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    height: 60px;
  }
  .empty {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
    padding: 24px;
    color: var(--fg-faint);
  }
  .empty p {
    margin: 0;
  }
  .empty .primary {
    min-height: 44px;
    padding: 8px 20px;
  }
</style>
