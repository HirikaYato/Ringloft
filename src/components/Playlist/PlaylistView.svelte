<script lang="ts">
  import { open, save } from '@tauri-apps/plugin-dialog';

  import ContextMenu, { type MenuItem } from '../Common/ContextMenu.svelte';
  import Prompt from '../Common/Prompt.svelte';
  import RowList from './RowList.svelte';
  import FileTools from './FileTools.svelte';
  import TagEditor from './TagEditor.svelte';
  import { formatTime, plural } from '../../lib/format';
  import { metrics } from '../../lib/typography.svelte.ts';
  import { api, type ColumnSetting, type PlaylistRow } from '../../lib/api';
  import { DEFAULT_COLUMNS, columnDef, gridTemplate } from '../../lib/columns';
  import { settings } from '../../stores/settings.svelte.ts';
  import ColumnHeader from './ColumnHeader.svelte';
  import { reorder, type ReorderState } from '../../lib/reorder';
  import PlaylistSidebar from './PlaylistSidebar.svelte';
  import IdentifyDialog from './IdentifyDialog.svelte';
  import { gain } from '../../stores/gain.svelte.ts';
  import { lyricsScan } from '../../stores/lyrics.svelte.ts';
  import { playlist } from '../../stores/playlist.svelte.ts';

  /** Высота строки одна на весь список — на ней держится вся виртуализация.
      Меняется вместе с размером шрифта. */
  const ROW_H = $derived(metrics.row);
  /** Сколько строк дорисовываем за краями экрана. */
  const PAD = 6;

  /**
   * Колонки — из настроек (`lib/columns.ts`). Пока край колонки тянут,
   * ширины живут в `liveColumns`; отпустили — уезжают в настройки.
   * В «#» показываем позицию в плейлисте, а сортируем по номеру трека из
   * тегов — так удобнее и листать, и собирать альбом по порядку.
   */
  /** Плейлисты списком слева вместо ленты вкладок. */
  const listed = $derived(settings.current?.ui.tabLayout === 'list');

  let liveColumns = $state<ColumnSetting[] | null>(null);
  const columns = $derived(liveColumns ?? settings.current?.ui.playlistColumns ?? DEFAULT_COLUMNS);
  const defs = $derived(columns.map((column) => columnDef(column.id)));
  const template = $derived(gridTemplate(columns));

  function changeColumns(next: ColumnSetting[], save: boolean) {
    liveColumns = next;
    if (save) void settings.patch({ ui: { playlistColumns: next } }).then(() => (liveColumns = null));
  }

  let viewport: HTMLDivElement | null = $state(null);
  let scrollTop = $state(0);
  let viewportHeight = $state(0);

  let renaming = $state<number | null>(null);
  /** Перестановка вкладок перетаскиванием (`lib/reorder.ts`). */
  let tabDrag = $state<ReorderState>({ dragging: null, indicator: null });
  const activeTab = $derived(playlist.tabs.find((tab) => tab.isActive));
  let showQueue = $state(false);
  // Открытие файлов и папки живёт в App: там же перетаскивание и общий
  // разбор путей, дублировать его здесь незачем.
  let {
    onopenfiles,
    onaddfolder,
    onhotkeys,
  }: { onopenfiles: () => void; onaddfolder: () => void; onhotkeys: () => void } = $props();

  let searchEl: HTMLInputElement | null = $state(null);
  let tabsEl: HTMLDivElement | null = $state(null);

  // Полоса прокрутки у ленты вкладок скрыта, а обычное колесо мыши крутит
  // вертикально — уехавшие вкладки мышью было не достать вовсе. Переводим
  // вертикальное колесо в горизонтальную прокрутку. Слушатель вешаем руками:
  // `preventDefault` работает только у непассивного.
  $effect(() => {
    const strip = tabsEl;
    if (!strip) return;
    const wheel = (event: WheelEvent) => {
      if (strip.scrollWidth <= strip.clientWidth) return;
      // Горизонтальный жест тачпада и так листает как надо.
      if (Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
      event.preventDefault();
      const step = event.deltaMode === WheelEvent.DOM_DELTA_LINE ? 40 : 1;
      strip.scrollLeft += event.deltaY * step;
    };
    strip.addEventListener('wheel', wheel, { passive: false });
    return () => strip.removeEventListener('wheel', wheel);
  });

  // Открытая вкладка всегда в поле ленты: и когда на неё переключились
  // (Alt+цифра, Ctrl+Tab), и когда ленту сжали или растянули — иначе после
  // смены ширины окна открытая вкладка могла остаться срезанной у края.
  // Смещение считаем сами: `scrollIntoView` тянул бы за собой и всё, что
  // выше по дереву.
  function revealActiveTab(smooth: boolean) {
    const strip = tabsEl;
    const id = playlist.tabs.find((tab) => tab.isActive)?.id;
    if (!strip || id === undefined) return;
    const tab = strip.querySelector<HTMLElement>(`[data-tab-id="${id}"]`);
    if (!tab) return;
    // Положение внутри ленты с учётом уже прокрученного.
    const left = tab.getBoundingClientRect().left - strip.getBoundingClientRect().left + strip.scrollLeft;
    const right = left + tab.offsetWidth;
    const behavior = smooth ? 'smooth' : 'instant';
    if (left < strip.scrollLeft) {
      strip.scrollTo({ left: left - 8, behavior });
    } else if (right > strip.scrollLeft + strip.clientWidth) {
      strip.scrollTo({ left: right - strip.clientWidth + 8, behavior });
    }
  }

  $effect(() => {
    // Зависимость от списка вкладок: смена открытой — повод доводить.
    void playlist.tabs.find((tab) => tab.isActive)?.id;
    revealActiveTab(true);
  });

  $effect(() => {
    const strip = tabsEl;
    if (!strip) return;
    const observer = new ResizeObserver(() => revealActiveTab(false));
    observer.observe(strip);
    return () => observer.disconnect();
  });
  // Ctrl+F живёт в App (там вся клавиатура), а поле здесь — поэтому просьба
  // приезжает счётчиком через стор.
  $effect(() => {
    if (playlist.searchFocus > 0) searchEl?.select();
  });

  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  /** Пути для редактора тегов; `null` — редактор закрыт. */
  let editing = $state<string[] | null>(null);
  /** Действия с файлами: корзина и переименование. `null` — диалог закрыт. */
  let fileTools = $state<{ mode: 'trash' | 'rename'; paths: string[] } | null>(null);
  let addingStream = $state(false);
  let dragFrom = $state<number | null>(null);
  let dragY = $state(0);
  let dropIndex = $state<number | null>(null);
  /** Вкладка под курсором при перетаскивании строк. */
  let dropTab = $state<number | null>(null);

  const searching = $derived(playlist.query.trim() !== '');
  const first = $derived(Math.max(0, Math.floor(scrollTop / ROW_H) - PAD));
  const last = $derived(
    Math.min(playlist.count, Math.ceil((scrollTop + viewportHeight) / ROW_H) + PAD),
  );
  const visible = $derived(
    Array.from({ length: Math.max(0, last - first) }, (_, nth) => first + nth),
  );

  // Прокрутили — досылаем запрос на недостающие строки.
  $effect(() => {
    if (playlist.count > 0) void playlist.ensureRange(first, last);
  });

  // Вход в поиск и выход из него пересоздают список: DOM начинает с нулевой
  // прокрутки, а состояние помнит прежнюю — строки рисуются далеко за экраном,
  // и список выглядит пустым. Сбрасываем прокрутку вместе с режимом.
  $effect(() => {
    void searching;
    scrollTop = 0;
    if (viewport) viewport.scrollTop = 0;
  });

  const IMPORT_FILTER = [{ name: 'Списки', extensions: ['m3u', 'm3u8', 'cue'] }];
  const EXPORT_FILTER = [{ name: 'Плейлисты', extensions: ['m3u8', 'm3u'] }];

  async function importList() {
    const selected = await open({ multiple: false, filters: IMPORT_FILTER });
    if (typeof selected === 'string') await playlist.importList(selected);
  }

  async function exportList() {
    const target = await save({
      defaultPath: 'playlist.m3u8',
      filters: EXPORT_FILTER,
    });
    if (typeof target === 'string') await playlist.exportM3u(target);
  }

  function indexFromY(clientY: number): number {
    if (!viewport) return 0;
    const rect = viewport.getBoundingClientRect();
    const offset = clientY - rect.top + viewport.scrollTop;
    return Math.max(0, Math.min(playlist.count, Math.round(offset / ROW_H)));
  }

  function rowPointerDown(event: PointerEvent, index: number) {
    if (event.button !== 0) return;
    const modifiers = { ctrl: event.ctrlKey || event.metaKey, shift: event.shiftKey };
    if (!playlist.selected.has(index) || modifiers.ctrl || modifiers.shift) {
      playlist.select(index, modifiers);
    }
    dragFrom = index;
    dragY = event.clientY;
    viewport?.focus();
  }

  /** Пути берём с бэкенда: выделение может уходить за пределы окна строк,
      а куски CUE он отсеет сам. */
  async function editTags() {
    const indexes = [...playlist.selected].sort((first, second) => first - second);
    try {
      const paths = await api.playlistPaths(indexes);
      if (paths.length > 0) editing = paths;
    } catch (err) {
      console.warn('[ringloft] не удалось собрать пути для правки тегов', err);
    }
  }

  /** Теги по звуку (AcoustID) — для выделения, пути с бэкенда. */
  let identifying = $state<string[] | null>(null);
  async function identifyTags() {
    const indexes = [...playlist.selected].sort((first, second) => first - second);
    try {
      const paths = await api.playlistPaths(indexes);
      if (paths.length > 0) identifying = paths;
    } catch (err) {
      console.warn('[ringloft] не удалось собрать пути для определения тегов', err);
    }
  }

  /** Текст ищется по выделению: пути снова с бэкенда. */
  async function scanLyrics() {
    const indexes = [...playlist.selected].sort((first, second) => first - second);
    try {
      const paths = await api.playlistPaths(indexes);
      if (paths.length > 0) await lyricsScan.scan(paths);
    } catch (err) {
      console.warn('[ringloft] не удалось собрать пути для поиска текста', err);
    }
  }

  /** Пути для действий с файлами: куски CUE и радио бэкенд отсеет сам. */
  async function openFileTools(mode: 'trash' | 'rename') {
    const indexes = [...playlist.selected].sort((first, second) => first - second);
    try {
      const paths = await api.playlistPaths(indexes);
      if (paths.length > 0) fileTools = { mode, paths };
    } catch (err) {
      console.warn('[ringloft] не удалось собрать пути для действий с файлами', err);
    }
  }

  /** Громкость считается по файлам, поэтому пути снова берём с бэкенда:
      выделение может уходить за окно строк. */
  async function scanGain() {
    const indexes = [...playlist.selected].sort((first, second) => first - second);
    try {
      const paths = await api.playlistPaths(indexes);
      if (paths.length > 0) await gain.scan(paths);
    } catch (err) {
      console.warn('[ringloft] не удалось собрать пути для подсчёта громкости', err);
    }
  }

  /** Адрес проверяем до добавления: иначе ошибка всплывёт только при
      попытке играть, и выглядеть будет непонятно. */
  function checkStreamUrl(value: string): string | null {
    let url: URL;
    try {
      url = new URL(value);
    } catch {
      return 'Это не похоже на адрес. Нужен полный, вида https://…';
    }
    if (url.protocol !== 'http:' && url.protocol !== 'https:') {
      return 'Плеер умеет только http и https.';
    }
    return null;
  }

  /** Редкие действия со списком — под одной кнопкой. */
  function moreMenu(event: MouseEvent) {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    menu = {
      x: rect.left,
      y: rect.bottom + 4,
      items: [
        { label: 'Добавить интернет-радио', action: () => (addingStream = true) },
        { separator: true },
        { label: 'Импорт M3U или CUE…', action: () => void importList() },
        {
          label: 'Экспорт в M3U8…',
          disabled: playlist.count === 0,
          action: () => void exportList(),
        },
        { separator: true },
        lyricsScan.running
          ? { label: 'Остановить поиск текстов', action: () => lyricsScan.cancel() }
          : {
              label: 'Найти тексты для списка',
              disabled: playlist.count === 0,
              action: () => void lyricsScan.scanPlaylist(),
            },
        gain.running
          ? { label: 'Остановить подсчёт громкости', action: () => gain.cancel() }
          : {
              label: 'Посчитать громкость списка',
              disabled: playlist.count === 0,
              action: () => void gain.scanPlaylist(),
            },
        { separator: true },
        {
          label: 'Папки списка…',
          disabled: !activeTab || activeTab.isTemporary,
          action: () => (playlist.sourcesFor = activeTab?.id ?? null),
        },
        {
          label: 'Обновить все списки по папкам',
          disabled: playlist.refreshing || !playlist.tabs.some((tab) => tab.sources > 0),
          action: () => void playlist.refreshSources(),
        },
        { separator: true },
        {
          label: 'Очистить список',
          disabled: playlist.count === 0,
          action: () => void playlist.clear(),
        },
        {
          label: 'Убрать пропавшие файлы',
          disabled: playlist.count === 0,
          action: () => void playlist.dropMissing(),
        },
        { separator: true },
        { label: 'Горячие клавиши (F1)', action: onhotkeys },
      ],
    };
  }

  function tabTitle(tab: (typeof playlist.tabs)[number]): string {
    const notes = [tab.name];
    if (tab.isTemporary) notes.push('Временная: файлы, открытые из проводника. Пересобирается при каждом открытии');
    if (tab.sources > 0) notes.push(`Следит за ${tab.sources} ${plural(tab.sources, 'папкой', 'папками', 'папками')}`);
    if (tab.isPlaying) notes.push('Отсюда играет музыка');
    return notes.join('\n');
  }

  /** Переименование и закрытие вкладки: крестик у ярлыка убран (на него
      слишком легко попасть мимо), поэтому закрывают отсюда. */
  function tabMenu(event: MouseEvent, id: number) {
    event.preventDefault();
    const tab = playlist.tabs.find((item) => item.id === id);
    menu = {
      x: event.clientX,
      y: event.clientY,
      items: [
        { label: 'Переименовать', action: () => (renaming = id) },
        { label: 'Новая вкладка', action: () => void playlist.createTab() },
        { separator: true },
        ...(tab?.isTemporary
          ? []
          : ([
              { label: 'Папки списка…', action: () => (playlist.sourcesFor = id) },
              {
                label: 'Обновить по папкам',
                disabled: !tab?.sources || playlist.refreshing,
                action: () => void playlist.refreshSources(id),
              },
              { separator: true },
            ] satisfies MenuItem[])),
        {
          label: 'Закрыть вкладку',
          disabled: playlist.tabs.length < 2,
          action: () => playlist.requestClose(id),
        },
      ],
    };
  }

  /** Правый клик по строке вне выделения работает как обычный клик: меню
      должно относиться к тому, на что нажали. */
  function rowMenu(event: MouseEvent, index: number, known?: PlaylistRow) {
    event.preventDefault();
    if (!playlist.selected.has(index)) playlist.select(index, { ctrl: false, shift: false });
    const row = known ?? playlist.row(index);
    const single = playlist.selected.size <= 1;
    const many = single ? '' : ` (${playlist.selected.size})`;
    // У радио нет ни файла, ни тегов: эти пункты для него бессмысленны.
    const isStream = row?.path.startsWith('http') ?? false;
    menu = {
      x: event.clientX,
      y: event.clientY,
      items: [
        { label: 'Играть', action: () => void playlist.play(index) },
        { label: `В очередь${many}`, action: () => playlist.enqueueSelected() },
        { separator: true },
        {
          label: `Изменить теги${many}`,
          disabled: !row || row.isRegion || isStream,
          action: () => void editTags(),
        },
        {
          label: `Определить теги по звуку${many}`,
          disabled: !row || row.isRegion || isStream,
          action: () => void identifyTags(),
        },
        {
          label: `Найти текст песни${many}`,
          disabled: !row || isStream || lyricsScan.running,
          action: () => void scanLyrics(),
        },
        {
          label: `Посчитать громкость${many}`,
          disabled: !row || isStream || gain.running,
          action: () => void scanGain(),
        },
        // Дальше — то, что меняет сами файлы на диске. Отдельной группой:
        // правка тегов и удаление в корзину в одном ряду читались как
        // равнозначные действия.
        { separator: true },
        {
          label: 'Открыть папку с треком',
          disabled: !row || isStream,
          action: () => {
            if (row) void api.revealInFolder(row.path);
          },
        },
        {
          label: `${single ? 'Переименовать файл' : 'Переименовать файлы'}${many}`,
          disabled: !row || row.isRegion || isStream,
          action: () => void openFileTools('rename'),
        },
        {
          label: `${single ? 'Удалить файл в корзину' : 'Удалить файлы в корзину'}${many}`,
          disabled: !row || row.isRegion || isStream,
          danger: true,
          action: () => void openFileTools('trash'),
        },
        { separator: true },
        { label: `Убрать из списка${many}`, action: () => void playlist.removeSelected() },
      ],
    };
  }

  function pointerMove(event: PointerEvent) {
    if (dragFrom === null) return;
    // Порог в 4 пикселя: обычный клик не должен превращаться в перетаскивание.
    if (dropIndex === null && dropTab === null && Math.abs(event.clientY - dragY) < 4) return;

    // Курсор над ярлыком другой вкладки — значит строки переносят туда, и
    // линию вставки внутри списка показывать уже не надо.
    const tab = tabUnder(event.clientX, event.clientY);
    dropTab = tab;
    dropIndex = tab === null ? indexFromY(event.clientY) : null;
  }

  /** Ярлык вкладки под точкой, кроме открытой: в неё переносить нечего. */
  function tabUnder(x: number, y: number): number | null {
    const element = document.elementFromPoint(x, y)?.closest<HTMLElement>('[data-tab-id]');
    const id = Number(element?.dataset.tabId);
    if (!id || playlist.tabs.find((tab) => tab.id === id)?.isActive) return null;
    return id;
  }

  function pointerUp(event: PointerEvent) {
    if (dropTab !== null) {
      // Ctrl — копия: так же, как Ctrl при перетаскивании файлов.
      void playlist.transferSelected(dropTab, event.ctrlKey || event.metaKey);
    } else if (dropIndex !== null) {
      void playlist.move(dropIndex);
    }
    dragFrom = null;
    dropIndex = null;
    dropTab = null;
  }

  function keyDown(event: KeyboardEvent) {
    const current = playlist.anchor ?? 0;
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      const next = Math.max(0, Math.min(playlist.count - 1, current + (event.key === 'ArrowDown' ? 1 : -1)));
      playlist.select(next, { ctrl: false, shift: event.shiftKey });
      scrollIntoView(next);
    } else if (event.key === 'Enter' && playlist.anchor !== null) {
      void playlist.play(playlist.anchor);
    } else if (event.key === 'Delete') {
      void playlist.removeSelected();
    } else if (event.key === 'a' && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      playlist.selectAll();
    }
  }

  function scrollIntoView(index: number) {
    if (!viewport) return;
    const top = index * ROW_H;
    if (top < viewport.scrollTop) viewport.scrollTop = top;
    else if (top + ROW_H > viewport.scrollTop + viewportHeight) {
      viewport.scrollTop = top + ROW_H - viewportHeight;
    }
  }
</script>

<svelte:window onpointermove={pointerMove} onpointerup={pointerUp} />

<div class="playlist" class:listed style:--columns={template}>
  {#if listed}
    <PlaylistSidebar bind:renaming {dropTab} oncontext={tabMenu} />
  {/if}
  <div class="main">
  <div class="toolbar">
    {#if !listed}
    <div
      class="tabs"
      bind:this={tabsEl}
      use:reorder={{
        axis: 'x',
        onmove: (id, before) => void playlist.moveTab(id, before),
        onstate: (state) => (tabDrag = state),
      }}
    >
    {#each playlist.tabs as tab (tab.id)}
      {#if tab.isTemporary}<span class="divider" aria-hidden="true"></span>{/if}
      {#if renaming === tab.id}
        <input
          class="tab-name"
          value={tab.name}
          onblur={(event) => {
            void playlist.renameTab(tab.id, event.currentTarget.value);
            renaming = null;
          }}
          onkeydown={(event) => {
            if (event.key === 'Enter') event.currentTarget.blur();
            if (event.key === 'Escape') renaming = null;
          }}
        />
      {:else}
        <button
          class="tab"
          class:active={tab.isActive}
          class:playing={tab.isPlaying}
          class:temporary={tab.isTemporary}
          class:drop={dropTab === tab.id}
          class:lifted={tabDrag.dragging === tab.id}
          data-tab-id={tab.id}
          data-fixed={tab.isTemporary ? '' : undefined}
          title={tabTitle(tab)}
          onclick={() => playlist.activateTab(tab.id)}
          ondblclick={() => (renaming = tab.id)}
          oncontextmenu={(event) => tabMenu(event, tab.id)}
        >
          {#if tab.isPlaying}<span class="dot"></span>{/if}
          {#if tab.isTemporary}
            <!-- Временная очередь: файлы из проводника, пересобирается при
                 каждом открытии. Отмечена значком и курсивом, чтобы не
                 сливаться с собранными списками. -->
            <svg class="tab-icon" viewBox="0 0 16 16" aria-hidden="true"><path d="M4 1.8h5.2L12.5 5v9.2H4zM9 1.8V5.2h3.5M6.2 8.5h4M6.2 11h2.6" /></svg>
          {/if}
          <span class="tab-label">{tab.name}</span>
          <span class="count">{tab.count}</span>
        </button>
      {/if}
    {/each}
      <button class="tab add" title="Новая вкладка" onclick={() => playlist.createTab()}>+</button>
      {#if tabDrag.indicator !== null}
        <div class="tab-indicator" style:left="{tabDrag.indicator - 1}px"></div>
      {/if}
    </div>
    {/if}

    <label class="search" title="Поиск по названию, исполнителю, альбому и имени файла">
      <svg viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"><circle cx="7" cy="7" r="4.2" /><path d="m10.2 10.2 3 3" /></svg>
      <input
        bind:this={searchEl}
        type="search"
        placeholder="Поиск"
        value={playlist.query}
        oninput={(event) => playlist.setQuery(event.currentTarget.value)}
        onkeydown={(event) => {
          if (event.key === 'Escape') {
            playlist.setQuery('');
            event.currentTarget.blur();
          }
        }}
      />
    </label>

    <div class="tools">
      {#if activeTab && activeTab.sources > 0}
        <button
          class="tab tool icon"
          class:spinning={playlist.refreshing}
          title="Обновить список по его папкам (правый клик — какие папки)"
          disabled={playlist.refreshing}
          onclick={() => void playlist.refreshSources(activeTab.id)}
          oncontextmenu={(event) => {
            event.preventDefault();
            playlist.sourcesFor = activeTab.id;
          }}
        >
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M13.2 8a5.2 5.2 0 1 1-1.5-3.7M13.2 2.4v2.9h-2.9" /></svg>
        </button>
      {/if}
      <button
        class="tab tool icon"
        title={listed ? 'Плейлисты вкладками сверху' : 'Плейлисты списком слева'}
        aria-label={listed ? 'Плейлисты вкладками сверху' : 'Плейлисты списком слева'}
        onclick={() => void settings.patch({ ui: { tabLayout: listed ? 'tabs' : 'list' } })}
      >
        {#if listed}
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2 4.5h12M2 4.5V13a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1V4.5M2 4.5V3a1 1 0 0 1 1-1h3.5v2.5" /></svg>
        {:else}
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 2.5h10a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1v-9a1 1 0 0 1 1-1M6 2.5v11M3.5 5h1M3.5 7h1M3.5 9h1" /></svg>
        {/if}
      </button>
      <button class="tab tool" onclick={onopenfiles}>Файлы</button>
      <button class="tab tool" onclick={onaddfolder}>Папка</button>
      <!-- Редкое — в меню: пять кнопок в строку отнимали у вкладок половину
           полосы. -->
      <button class="tab tool more" title="Ещё" onclick={moreMenu}>⋯</button>
    </div>
  </div>

  <!-- Заголовок и в поиске: колонки те же, а сортировка списка меняет и
       порядок результатов. -->
  <ColumnHeader
    {columns}
    sort={playlist.sort}
    onsort={(key) => playlist.sortBy(key)}
    onchange={changeColumns}
  />

  {#if searching}
    <div class="viewport results">
      <RowList
        rows={playlist.found}
        {columns}
        empty="Ничего не нашлось."
        onactivate={(row) => playlist.play(row.index)}
        oncontext={(event, row) => rowMenu(event, row.index, row)}
      />
      {#if playlist.foundTotal > playlist.found.length}
        <p class="more">
          Нашлось {playlist.foundTotal}, показаны первые {playlist.found.length}.
        </p>
      {/if}
    </div>
  {:else}
  <div
    class="viewport"
    bind:this={viewport}
    bind:clientHeight={viewportHeight}
    tabindex="0"
    role="listbox"
    aria-label="Плейлист"
    aria-multiselectable="true"
    onscroll={(event) => (scrollTop = event.currentTarget.scrollTop)}
    onkeydown={keyDown}
  >
    {#if playlist.count === 0}
      <p class="empty">Плейлист пуст. Открой файлы, добавь папку или перетащи их в окно.</p>
    {:else}
      <div class="spacer" style:height="{playlist.count * ROW_H}px">
        {#each visible as index (index)}
          {@const row = playlist.row(index)}
          <div
            class="row"
            class:selected={playlist.selected.has(index)}
            class:current={index === playlist.currentIndex}
            class:pending={row ? !row.metaLoaded : true}
            style:top="{index * ROW_H}px"
            role="option"
            aria-selected={playlist.selected.has(index)}
            tabindex="-1"
            onpointerdown={(event) => rowPointerDown(event, index)}
            oncontextmenu={(event) => rowMenu(event, index)}
            ondblclick={() => playlist.play(index)}
          >
            {#each defs as def (def.id)}
              <span class="cell kind-{def.kind}">
                {row ? def.value(row, index) : def.id === 'title' ? '…' : ''}
              </span>
            {/each}
          </div>
        {/each}

        {#if dropIndex !== null}
          <div class="drop-line" style:top="{dropIndex * ROW_H}px"></div>
        {/if}
      </div>
    {/if}
  </div>
  {/if}

  {#if showQueue && playlist.queueRows.length > 0}
    <div class="queue">
      {#each playlist.queueRows as row, index (row.id)}
        <div class="queue-row">
          <span class="num">{index + 1}</span>
          <span class="name">{row.title}</span>
          <span class="artist">{row.artist ?? ''}</span>
        </div>
      {/each}
    </div>
  {/if}

  <div class="foot">
    {#if searching}
      <span>нашлось {playlist.foundTotal} из {playlist.count}</span>
    {:else}
      <span>{playlist.count} {plural(playlist.count, 'трек', 'трека', 'треков')}</span>
    {/if}
    {#if playlist.totalDurationMs > 0}
      <span class="dim">· {formatTime(playlist.totalDurationMs)}</span>
    {/if}
    {#if playlist.selected.size > 0}
      <span class="dim">· выделено {playlist.selected.size}</span>
      <button class="foot-button" onclick={() => playlist.enqueueSelected()}>В очередь</button>
    {/if}
    <span class="spacer"></span>
    {#if playlist.queued > 0}
      <button
        class="foot-button"
        onclick={() => {
          showQueue = !showQueue;
          if (showQueue) void playlist.loadQueue();
        }}
      >
        очередь: {playlist.queued}
      </button>
      <button class="foot-button" onclick={() => playlist.clearQueue()}>очистить</button>
    {/if}
  </div>
  </div>
</div>

{#if fileTools}
  <FileTools
    mode={fileTools.mode}
    paths={fileTools.paths}
    onclose={() => (fileTools = null)}
  />
{/if}

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

{#if identifying}
  <IdentifyDialog paths={identifying} onclose={() => (identifying = null)} />
{/if}

{#if editing}
  <TagEditor paths={editing} onclose={() => (editing = null)} />
{/if}

{#if addingStream}
  <Prompt
    title="Интернет-радио"
    label="Адрес потока"
    placeholder="https://example.org/stream"
    hint="Поток добавится строкой в список. Перемотки у радио нет, название приходит из эфира."
    validate={checkStreamUrl}
    onsubmit={(url) => void playlist.add([url])}
    onclose={() => (addingStream = false)}
  />
{/if}

<style>
  .playlist {
    display: flex;
    flex-direction: row;
    flex: 1;
    min-height: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-elev);
    overflow: hidden;
  }
  /* Полоса не прокручивается целиком: поиск и кнопки всегда на месте,
     а уезжать вбок могут только вкладки. */
  /* Сам список с полосой инструментов; слева от него — колонка плейлистов,
     если включён вид списком. */
  .main {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    min-height: 0;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 6px 0;
    background: var(--bg-inset);
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  .tabs {
    position: relative;
    display: flex;
    align-items: flex-end;
    gap: 2px;
    flex: 1 1 auto;
    min-width: 0;
    overflow-x: auto;
    /* Горизонтальная полоса прокрутки поверх вкладок выглядит хуже, чем
       обрезанное название: листаем колесом. */
    scrollbar-width: none;
  }
  .tabs::-webkit-scrollbar {
    display: none;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 2px;
    flex: none;
  }
  .tab-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tab {
    display: flex;
    align-items: center;
    gap: 6px;
    /* Сжимаются, но не до неузнаваемости: когда места нет совсем, полоса
       уезжает вбок — это честнее, чем название из одной буквы. */
    flex: 0 1 auto;
    min-width: calc(var(--fs-md) * 8.7);
    max-width: calc(var(--fs-md) * 12.7);
    font: inherit;
    font-size: var(--fs-sm);
    color: var(--fg-dim);
    background: none;
    border: none;
    border-radius: 6px 6px 0 0;
    padding: 5px 9px;
    cursor: pointer;
    white-space: nowrap;
  }
  .tab:hover {
    color: var(--fg);
  }
  .tab:hover:not(.active) {
    background: color-mix(in srgb, var(--bg-elev) 55%, transparent);
  }
  /* Открытая вкладка просто сливается с содержимым под ней: рамка вокруг
     надписи выглядела как ошибка вёрстки. */
  .tab.active {
    background: var(--bg-elev);
    color: var(--fg);
  }
  /* Вкладка под перетаскиваемыми строками: акцентом, чтобы было видно, куда
     они уедут. */
  .tab.drop {
    background: var(--accent-soft);
    color: var(--fg);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  /* Играющую отмечаем точкой, а не второй рамкой: «открыта» и «звучит» —
     разные вещи, и путать их не надо. */
  .tab .dot {
    width: 6px;
    height: 6px;
    flex: none;
    border-radius: 50%;
    background: var(--accent);
  }
  .tab .count {
    font-family: var(--font-mono);
    font-size: calc(var(--fs-xs) - 1px);
    color: var(--fg-faint);
  }
  /* Временная вкладка отделена чертой и написана курсивом: это очередь
     «вот эти файлы», а не собранный список. */
  .divider {
    flex: none;
    width: 1px;
    height: 16px;
    margin: 0 4px;
    align-self: center;
    background: var(--border);
  }
  .tab.temporary {
    font-style: italic;
  }
  .tab.lifted {
    opacity: 0.45;
  }
  /* Куда встанет перетаскиваемая вкладка. */
  .tab-indicator {
    position: absolute;
    top: 4px;
    bottom: 2px;
    width: 2px;
    border-radius: 1px;
    background: var(--accent);
    pointer-events: none;
  }
  .tab-icon,
  .tab.icon svg {
    width: 14px;
    height: 14px;
    flex: none;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.4;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .tab.icon.spinning svg {
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .tab.add {
    flex: none;
    min-width: 0;
    padding: 4px 10px;
    color: var(--fg-faint);
  }
  /* Вид списком: ленты вкладок нет — поиск шире, инструменты прижаты
     вправо, а полоса не ждёт вкладок снизу. */
  /* По высоте — как шапка колонки плейлистов: нижние линии сходятся. */
  .listed .toolbar {
    min-height: var(--chrome-h);
    box-sizing: border-box;
    padding: 0 8px;
  }
  .listed .search {
    flex: 1 1 320px;
    max-width: 480px;
  }
  .listed .tools {
    margin-left: auto;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 7px;
    /* Не растём за счёт вкладок: их читать важнее, чем иметь широкое поле. */
    flex: 0 1 240px;
    min-width: 120px;
    padding: 0 11px;
    height: calc(var(--fs-md) + 15px);
    color: var(--fg-faint);
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: 999px;
  }
  .search:focus-within {
    border-color: color-mix(in srgb, var(--accent) 60%, var(--border));
  }
  .search svg {
    width: 14px;
    height: 14px;
    flex: none;
  }
  .search input {
    min-width: 0;
    width: 100%;
    font: inherit;
    color: var(--fg);
    background: none;
    border: none;
    outline: none;
  }
  .search input::placeholder {
    color: var(--fg-faint);
  }
  /* Нативный крестик у `type=search` в WebKitGTK не попадает в тему. */
  .search input::-webkit-search-cancel-button {
    display: none;
  }
  /* Без своих отступов: колонки результатов стоят ровно под заголовком. */
  .results {
    padding: 0;
  }
  .more {
    margin: 8px 2px 2px;
    font-size: var(--fs-xs);
    color: var(--fg-faint);
  }
  .head.hidden {
    display: none;
  }
  .tab.more {
    font-size: 17px;
    line-height: 1;
    padding: 4px 9px;
  }
  .tab.tool {
    flex: none;
    min-width: 0;
    font-size: var(--fs-sm);
    color: var(--fg-dim);
  }
  .tab.tool:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .tab-name {
    font: inherit;
    font-size: var(--fs-sm);
    width: 130px;
    color: var(--fg);
    background: var(--bg-elev);
    border: 1px solid var(--accent);
    border-radius: 6px 6px 0 0;
    padding: 4px 8px;
  }
  .queue {
    max-height: 140px;
    overflow-y: auto;
    border-top: 1px solid var(--border);
    background: var(--bg-inset);
    flex: none;
  }
  .queue-row {
    display: grid;
    grid-template-columns: 32px 1fr 1fr;
    gap: 8px;
    padding: 2px 10px;
    font-size: var(--fs-xs);
    color: var(--fg-dim);
  }
  .queue-row .name {
    color: var(--fg);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .queue-row .artist {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .foot-button {
    font: inherit;
    font-size: var(--fs-xs);
    color: var(--fg-dim);
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 1px 8px;
    margin-left: 6px;
    cursor: pointer;
  }
  .foot-button:hover {
    color: var(--fg);
    background: var(--row-hover);
  }
  .spacer {
    flex: 1;
  }
  .viewport {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    outline: none;
  }
  .viewport:focus-visible {
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .spacer {
    position: relative;
  }
  .row {
    position: absolute;
    left: 0;
    right: 0;
    height: var(--row-h);
    display: grid;
    grid-template-columns: var(--columns);
    align-items: center;
    font-size: var(--fs-sm);
    cursor: default;
  }
  .row:nth-child(even) {
    background: var(--row-alt);
  }
  .row:hover {
    background: var(--row-hover);
  }
  .row.selected {
    background: var(--row-sel);
  }
  .row.current {
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .row.current .kind-title {
    color: var(--accent);
    font-weight: 600;
  }
  .row.pending .kind-title {
    color: var(--fg-faint);
  }
  .cell {
    padding: 0 8px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .kind-mono {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--fg-faint);
    text-align: right;
  }
  .kind-dim {
    color: var(--fg-dim);
  }
  .kind-stars {
    font-size: var(--fs-xs);
    letter-spacing: 0.08em;
    color: color-mix(in srgb, var(--accent) 80%, var(--fg-dim));
    text-align: right;
  }
  .drop-line {
    position: absolute;
    left: 0;
    right: 0;
    height: 2px;
    background: var(--accent);
    pointer-events: none;
  }
  .empty {
    margin: 0;
    padding: 24px;
    text-align: center;
    color: var(--fg-faint);
    font-size: var(--fs-sm);
  }
  .foot {
    display: flex;
    gap: 4px;
    align-items: center;
    padding: 4px 10px;
    border-top: 1px solid var(--border);
    background: var(--bg-inset);
    font-size: var(--fs-xs);
    color: var(--fg-dim);
    flex: none;
  }
  .dim {
    color: var(--fg-faint);
  }
</style>
