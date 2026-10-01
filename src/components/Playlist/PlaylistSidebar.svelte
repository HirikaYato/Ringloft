<script lang="ts">
  /**
   * Плейлисты колонкой слева — второй вид ленты вкладок (`ui.tabLayout`).
   * Умеет то же, что лента: переключить, переименовать двойным щелчком, меню
   * по правому клику, принять перетащенные строки (`data-tab-id`), поменять
   * порядок перетаскиванием. Ширина тянется за правый край; потянули совсем
   * узко — колонка сворачивается в полосу с буквами.
   */
  import { plural } from '../../lib/format';
  import { reorder, type ReorderState } from '../../lib/reorder';
  import { playlist } from '../../stores/playlist.svelte.ts';
  import { settings } from '../../stores/settings.svelte.ts';

  let {
    renaming = $bindable(),
    dropTab,
    oncontext,
  }: {
    renaming: number | null;
    dropTab: number | null;
    oncontext: (event: MouseEvent, id: number) => void;
  } = $props();

  type Tab = (typeof playlist.tabs)[number];

  const DEFAULT_WIDTH = 220;
  const COLLAPSE_BELOW = 130;

  const regular = $derived(playlist.tabs.filter((tab) => !tab.isTemporary));
  const temporary = $derived(playlist.tabs.filter((tab) => tab.isTemporary));
  const collapsed = $derived(settings.current?.ui.sidebarCollapsed ?? false);
  let liveWidth = $state<number | null>(null);
  const width = $derived(liveWidth ?? settings.current?.ui.sidebarWidth ?? DEFAULT_WIDTH);

  let drag = $state<ReorderState>({ dragging: null, indicator: null });

  function tip(tab: Tab): string {
    const parts = [`${tab.name} — ${tab.count} ${plural(tab.count, 'трек', 'трека', 'треков')}`];
    if (tab.isPlaying) parts.push('отсюда играет музыка');
    if (tab.sources > 0) parts.push('следит за папками');
    if (tab.isTemporary) parts.push('временная: файлы, открытые из проводника');
    return parts.join('\n');
  }

  /** Буквы для свёрнутой полосы: «Мой плейлист» → «МП», «All» → «Al». */
  function initials(name: string): string {
    const words = name.trim().split(/\s+/).filter(Boolean);
    if (words.length >= 2) return (words[0]![0]! + words[1]![0]!).toUpperCase();
    return (words[0] ?? '?').slice(0, 2);
  }

  /** Оттенок значка от названия — чтобы списки различались и в полосе. */
  function hue(name: string): number {
    let hash = 0;
    for (const ch of name) hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
    return hash % 360;
  }

  function setCollapsed(value: boolean) {
    void settings.patch({ ui: { sidebarCollapsed: value } });
  }

  // Ширина: тянем правый край. Узко — сворачиваем, двойной щелчок — сброс.
  let resizing: { startX: number; startWidth: number } | null = null;

  function startResize(event: PointerEvent) {
    event.preventDefault();
    resizing = { startX: event.clientX, startWidth: collapsed ? 52 : width };
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function resize(event: PointerEvent) {
    if (!resizing) return;
    const next = Math.round(resizing.startWidth + event.clientX - resizing.startX);
    if (collapsed && next > COLLAPSE_BELOW) setCollapsed(false);
    liveWidth = Math.min(480, Math.max(160, next));
    if (next < COLLAPSE_BELOW && !collapsed) {
      liveWidth = null;
      setCollapsed(true);
      resizing = null;
    }
  }

  function endResize() {
    if (liveWidth !== null) {
      const saved = liveWidth;
      void settings.patch({ ui: { sidebarWidth: saved } }).then(() => (liveWidth = null));
    }
    resizing = null;
  }
</script>

{#snippet entry(tab: Tab)}
  {#if renaming === tab.id && !collapsed}
    <!-- svelte-ignore a11y_autofocus -->
    <input
      class="input rename"
      value={tab.name}
      autofocus
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
      class="entry"
      class:active={tab.isActive}
      class:playing={tab.isPlaying}
      class:drop={dropTab === tab.id}
      class:lifted={drag.dragging === tab.id}
      class:temporary={tab.isTemporary}
      data-tab-id={tab.id}
      data-fixed={tab.isTemporary ? '' : undefined}
      title={tip(tab)}
      onclick={() => playlist.activateTab(tab.id)}
      ondblclick={() => (renaming = tab.id)}
      oncontextmenu={(event) => oncontext(event, tab.id)}
    >
      {#if collapsed}
        <span class="badge" style:--hue={hue(tab.name)}>
          {initials(tab.name)}
          {#if tab.isPlaying}<span class="pip"></span>{/if}
        </span>
      {:else}
        <span class="icon" aria-hidden="true">
          {#if tab.isPlaying}
            <span class="bars"><i></i><i></i><i></i></span>
          {:else if tab.isTemporary}
            <svg viewBox="0 0 16 16"><path d="M4 1.8h5.2L12.5 5v9.2H4zM9 1.8V5.2h3.5" /></svg>
          {:else}
            <svg viewBox="0 0 16 16"><path d="M2.5 4h8M2.5 7.5h8M2.5 11h5M13 3v7.5" /><circle cx="11.6" cy="11" r="1.6" /></svg>
          {/if}
        </span>
        <span class="name">{tab.name}</span>
        {#if tab.sources > 0}<span class="sources" title="Следит за папками">↻</span>{/if}
        <span class="count">{tab.count}</span>
      {/if}
    </button>
  {/if}
{/snippet}

<aside class="sidebar" class:collapsed style:width="{collapsed ? 52 : width}px" aria-label="Плейлисты">
  <header>
    {#if !collapsed}<span class="title">Плейлисты</span>{/if}
    <button
      class="fold"
      title={collapsed ? 'Развернуть колонку' : 'Свернуть колонку'}
      aria-label={collapsed ? 'Развернуть колонку' : 'Свернуть колонку'}
      onclick={() => setCollapsed(!collapsed)}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d={collapsed ? 'M6 3.5 10.5 8 6 12.5' : 'M10 3.5 5.5 8 10 12.5'} /></svg>
    </button>
  </header>

  <nav
    class="entries"
    use:reorder={{
      axis: 'y',
      onmove: (id, before) => void playlist.moveTab(id, before),
      onstate: (state) => (drag = state),
    }}
  >
    {#each regular as tab (tab.id)}{@render entry(tab)}{/each}
    {#if drag.indicator !== null}
      <div class="indicator" style:top="{drag.indicator - 1}px"></div>
    {/if}
  </nav>

  {#if temporary.length > 0}
    <!-- Временная очередь — отдельно от собранных списков. -->
    <div class="entries temp">
      {#if !collapsed}<span class="caption">Временные</span>{/if}
      {#each temporary as tab (tab.id)}{@render entry(tab)}{/each}
    </div>
  {/if}

  <footer>
    <button class="new" title="Новый плейлист (Ctrl+T)" onclick={() => playlist.createTab()}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 3v10M3 8h10" /></svg>
      {#if !collapsed}<span>Новый плейлист</span>{/if}
    </button>
  </footer>

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="resizer"
    title="Потянуть — ширина колонки, двойной щелчок — по умолчанию"
    onpointerdown={startResize}
    onpointermove={resize}
    onpointerup={endResize}
    onpointercancel={endResize}
    ondblclick={() => {
      setCollapsed(false);
      void settings.patch({ ui: { sidebarWidth: DEFAULT_WIDTH } });
    }}
  ></div>
</aside>

<style>
  .sidebar {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: none;
    min-height: 0;
    background: var(--bg-inset);
    border-right: 1px solid var(--border);
  }
  /* Высота шапки — как у полосы инструментов справа: линии сходятся. */
  header {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: var(--chrome-h);
    padding: 0 6px 0 14px;
    border-bottom: 1px solid var(--border);
    box-sizing: border-box;
  }
  .collapsed header {
    justify-content: center;
    padding: 0;
  }
  .title {
    flex: 1;
    font-size: var(--fs-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--fg-faint);
  }
  .fold,
  .new {
    display: grid;
    place-items: center;
    padding: 0;
    color: var(--fg-faint);
    background: none;
    border: 0;
    border-radius: var(--radius);
    cursor: pointer;
  }
  .fold {
    width: 28px;
    height: 28px;
  }
  .fold:hover,
  .new:hover {
    color: var(--fg);
    background: var(--row-hover);
  }
  svg {
    width: 16px;
    height: 16px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.4;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .entries {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 8px 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    scrollbar-width: thin;
  }
  .entries.temp {
    flex: none;
    max-height: 30%;
    border-top: 1px solid var(--border);
  }
  .caption {
    padding: 2px 8px 4px;
    font-size: var(--fs-xs);
    color: var(--fg-faint);
  }
  .entry {
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
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
    transition:
      background var(--dur-fast) var(--ease),
      color var(--dur-fast) var(--ease);
  }
  .entry:hover {
    color: var(--fg);
    background: var(--row-hover);
  }
  /* Открытый список — заливка и акцентная черта слева, как у строки
     текущего трека: «где я» читается одинаково по всему окну. */
  .entry.active {
    color: var(--fg);
    background: var(--accent-soft);
  }
  .entry.active::before {
    content: '';
    position: absolute;
    left: 0;
    top: 22%;
    bottom: 22%;
    width: 3px;
    border-radius: 2px;
    background: var(--accent);
  }
  .entry.drop {
    color: var(--fg);
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .entry.lifted {
    opacity: 0.45;
  }
  .entry.temporary .name {
    font-style: italic;
  }
  .icon {
    display: grid;
    place-items: center;
    width: 16px;
    flex: none;
    color: var(--fg-faint);
  }
  .entry.active .icon {
    color: var(--accent);
  }
  .icon circle {
    fill: none;
  }
  /* Играющий список — три «прыгающих» столбика вместо значка. */
  .bars {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    height: 12px;
  }
  .bars i {
    width: 3px;
    height: 100%;
    border-radius: 1px;
    background: var(--accent);
    transform-origin: bottom;
    animation: bounce 1s ease-in-out infinite;
  }
  .bars i:nth-child(2) {
    animation-delay: -0.35s;
  }
  .bars i:nth-child(3) {
    animation-delay: -0.7s;
  }
  @keyframes bounce {
    0%,
    100% {
      transform: scaleY(0.35);
    }
    50% {
      transform: scaleY(1);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .bars i {
      animation: none;
    }
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sources {
    font-size: var(--fs-xs);
    color: var(--fg-faint);
  }
  .count {
    min-width: 2.2em;
    padding: 1px 6px;
    font-family: var(--font-mono);
    font-size: calc(var(--fs-xs) - 1px);
    text-align: center;
    color: var(--fg-faint);
    background: color-mix(in srgb, var(--fg) 6%, transparent);
    border-radius: 999px;
  }
  /* Свёрнутая полоса: буквы в цветном квадрате, подпись — во всплывающей
     подсказке. */
  .collapsed .entries {
    padding: 8px 6px;
    align-items: center;
  }
  .collapsed .entry {
    justify-content: center;
    width: 40px;
    min-height: 40px;
    padding: 0;
  }
  .collapsed .entry.active::before {
    left: -6px;
  }
  .badge {
    position: relative;
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    font-size: var(--fs-xs);
    font-weight: 700;
    color: hsl(var(--hue) 55% 78%);
    background: hsl(var(--hue) 35% 26%);
    border-radius: 8px;
  }
  :global([data-theme='light']) .badge {
    color: hsl(var(--hue) 45% 30%);
    background: hsl(var(--hue) 55% 88%);
  }
  .pip {
    position: absolute;
    right: -3px;
    top: -3px;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 0 2px var(--bg-inset);
  }
  .indicator {
    position: absolute;
    left: 8px;
    right: 8px;
    height: 2px;
    border-radius: 1px;
    background: var(--accent);
    pointer-events: none;
  }
  footer {
    padding: 8px;
    border-top: 1px solid var(--border);
  }
  .new {
    display: flex;
    align-items: center;
    justify-content: flex-start;
    gap: 8px;
    width: 100%;
    min-height: 32px;
    padding: 0 10px;
    font: inherit;
    font-size: var(--fs-sm);
  }
  .collapsed .new {
    justify-content: center;
    padding: 0;
  }
  .rename {
    margin: 2px 0;
    font-size: var(--fs-sm);
  }
  /* Тянется за правую границу. */
  .resizer {
    position: absolute;
    top: 0;
    right: -4px;
    bottom: 0;
    width: 8px;
    z-index: 2;
    cursor: col-resize;
    touch-action: none;
  }
  .resizer:hover {
    background: color-mix(in srgb, var(--accent) 30%, transparent);
  }
</style>
