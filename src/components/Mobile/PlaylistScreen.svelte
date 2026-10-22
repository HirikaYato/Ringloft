<script lang="ts">
  import { flushSync, untrack } from 'svelte';

  import { api, type PlaylistRow } from '../../lib/api';
  import { forgetBack, pushBack } from '../../lib/back';
  import { hideKeyboardOnEnter } from '../../lib/keyboard.svelte.ts';
  import { playlist } from '../../stores/playlist.svelte.ts';
  import Icon from './Icon.svelte';
  import PlaylistMenus, { type PlaylistSheet } from './PlaylistMenus.svelte';
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

  /**
   * `shown` — раздел на экране: «назад» выходит из поиска и выделения,
   * только пока он виден. `reveal` растёт, когда просят встать на играющую
   * строку (из плеера во весь экран).
   */
  const { shown, reveal = 0 }: { shown: boolean; reveal?: number } = $props();

  let viewport = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let height = $state(0);
  let searching = $state(false);
  let searchInput = $state<HTMLInputElement | null>(null);
  let picked = $state<Set<number>>(new Set());
  let sheet = $state<PlaylistSheet>('none');

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

  // Встать на играющую строку. Вкладку перед этим могли переключить, а её
  // строки и номер играющей приезжают позже — поэтому просьба ждёт их здесь.
  // Объявлено после сброса прокрутки: в одном проходе он сработает раньше.
  let revealing = $state(false);
  $effect(() => {
    if (reveal === 0) return;
    untrack(() => {
      if (searching) stopSearch();
      picked = new Set();
      revealing = true;
    });
  });
  $effect(() => {
    void activeId;
    const index = playlist.currentIndex;
    if (!revealing || index === null || !active?.isPlaying || !viewport || height === 0) return;
    revealing = false;
    const top = Math.max(0, index * ROW - (height - ROW) / 2);
    viewport.scrollTop = top;
    scrollTop = top;
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

  /**
   * Фокус ставим сразу, в самом обработчике касания: клавиатуру вебвью
   * открывает только на фокус из жеста пользователя, а `autofocus` у поля,
   * которое появилось после касания, срабатывает уже без него.
   */
  function startSearch() {
    searching = true;
    flushSync();
    searchInput?.focus();
  }

  function stopSearch() {
    searching = false;
    playlist.setQuery('');
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
      <input
        bind:this={searchInput}
        class="input search"
        type="search"
        placeholder="Название, исполнитель, альбом"
        value={playlist.query}
        oninput={(event) => playlist.setQuery(event.currentTarget.value)}
        onkeydown={hideKeyboardOnEnter}
      />
    </header>
  {:else}
    <header>
      <button class="name" onclick={() => (sheet = 'lists')}>
        <span>{active?.name ?? 'Плейлист'}</span>
        <Icon name="down" size={18} />
      </button>
      <button class="key" aria-label="Поиск" onclick={startSearch}><Icon name="search" /></button>
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

<PlaylistMenus bind:sheet />

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
