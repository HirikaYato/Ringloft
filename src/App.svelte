<script lang="ts">
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { open } from '@tauri-apps/plugin-dialog';
  import { onDestroy, onMount } from 'svelte';

  import ContextMenu, { type MenuItem } from './components/Common/ContextMenu.svelte';
  import ErrorBar from './components/Common/ErrorBar.svelte';
  import GainBar from './components/Common/GainBar.svelte';
  import LyricsBar from './components/Common/LyricsBar.svelte';
  import CoversBar from './components/Common/CoversBar.svelte';
  import { covers } from './stores/covers.svelte.ts';
  import ResizeEdges from './components/Common/ResizeEdges.svelte';
  import WindowButtons from './components/Common/WindowButtons.svelte';
  import LibraryBrowser from './components/Library/LibraryBrowser.svelte';
  import LibraryPanel from './components/Library/LibraryPanel.svelte';
  import Equalizer from './components/Player/Equalizer.svelte';
  import HeadphoneEq from './components/Player/HeadphoneEq.svelte';
  import FullView from './components/Player/FullView.svelte';
  import Lyrics from './components/Player/Lyrics.svelte';
  import MiniBar from './components/Player/MiniBar.svelte';
  import PlayerBar from './components/Player/PlayerBar.svelte';
  import CloseTabDialog from './components/Playlist/CloseTabDialog.svelte';
  import SourcesDialog from './components/Playlist/SourcesDialog.svelte';
  import Notice from './components/Common/Notice.svelte';
  import PlaylistView from './components/Playlist/PlaylistView.svelte';
  import UpNext from './components/Playlist/UpNext.svelte';
  import Hotkeys from './components/Common/Hotkeys.svelte';
  import Modal from './components/Common/Modal.svelte';
  import SettingsPanel from './components/Settings/SettingsPanel.svelte';
  import { api, toErrorPayload, type AppInfo, type ErrorPayload } from './lib/api';
  import { findHotkey, type HotkeyId } from './lib/hotkeys';
  import { applyTypography } from './lib/typography.svelte.ts';
  import { gain } from './stores/gain.svelte.ts';
  import { library } from './stores/library.svelte.ts';
  import { lyricsScan } from './stores/lyrics.svelte.ts';
  import { player } from './stores/player.svelte.ts';
  import { sleep } from './stores/sleep.svelte.ts';
  import { playlist } from './stores/playlist.svelte.ts';
  import { accentFromTint } from './lib/cover-color';
  import { applyAccent, applyTheme, resolvedTheme, settings } from './stores/settings.svelte.ts';
  import { appWindow, applyAlwaysOnTop, applyMiniWindow } from './lib/window';

  const AUDIO_EXTENSIONS = [
    'mp3', 'mp2', 'flac', 'wav', 'wave', 'ogg', 'oga', 'opus', 'weba', 'm4a', 'm4b', 'mp4', 'aac',
    'mka', 'aiff', 'aif',
  ];

  /** Дополнительные панели взаимоисключающие: иначе втроём они съедают окно. */
  type Panel = 'none' | 'upnext' | 'eq' | 'lyrics' | 'settings' | 'keys';

  let info = $state<AppInfo | null>(null);
  let bootError = $state<ErrorPayload | null>(null);
  let dropActive = $state(false);
  let view = $state<'playlist' | 'library'>('playlist');
  let panel = $state<Panel>('none');
  let fullView = $state(false);
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  const PANELS: { id: Panel; label: string; icon: string }[] = [
    {
      id: 'upnext',
      label: 'Дальше по порядку',
      icon: 'M2.5 4h8M2.5 7.5h8M2.5 11h5M12 9.2l3.2 2-3.2 2z',
    },
    { id: 'eq', label: 'Эквалайзер', icon: 'M4 2v4M4 10v4M9 2v7M9 13v1M14 2v2M14 8v6' },
    {
      id: 'lyrics',
      label: 'Текст песни',
      icon: 'M3.5 2.5h9a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1v-9a1 1 0 0 1 1-1M5 5.5h6M5 8h6M5 10.5h3.5',
    },
    {
      id: 'settings',
      label: 'Настройки',
      // Шестерёнка: прежний кружок с лучами читался как «сменить тему».
      icon: 'M6.65 2.98L6.92 1.18L9.08 1.18L9.35 2.98A5.2 5.2 0 0 1 10.60 3.50L12.06 2.42L13.58 3.94L12.50 5.40A5.2 5.2 0 0 1 13.02 6.65L14.82 6.92L14.82 9.08L13.02 9.35A5.2 5.2 0 0 1 12.50 10.60L13.58 12.06L12.06 13.58L10.60 12.50A5.2 5.2 0 0 1 9.35 13.02L9.08 14.82L6.92 14.82L6.65 13.02A5.2 5.2 0 0 1 5.40 12.50L3.94 13.58L2.42 12.06L3.50 10.60A5.2 5.2 0 0 1 2.98 9.35L1.18 9.08L1.18 6.92L2.98 6.65A5.2 5.2 0 0 1 3.50 5.40L2.42 3.94L3.94 2.42L5.40 3.50A5.2 5.2 0 0 1 6.65 2.98ZM10.3 8A2.3 2.3 0 1 1 5.7 8A2.3 2.3 0 1 1 10.3 8',
    },
  ];

  function togglePanel(next: Panel) {
    panel = panel === next ? 'none' : next;
  }

  /** Раздел запоминается между запусками. */
  function switchView(next: 'playlist' | 'library') {
    view = next;
    void settings.patch({ ui: { view: next } });
  }

  /**
   * Правый клик по полосе заголовка: сначала просим меню у рабочего стола
   * (у KDE там и «на все рабочие столы», и «поверх остальных»), а если он
   * отказал — показываем своё с тем, что умеем сами.
   */
  async function windowMenu(event: MouseEvent) {
    event.preventDefault();
    const point = { x: event.clientX, y: event.clientY };
    try {
      if (await api.windowShowMenu()) return;
    } catch (err) {
      console.warn('[ringloft] системное меню окна недоступно', err);
    }

    const win = appWindow();
    const pinned = settings.current?.ui.alwaysOnTop ?? false;
    menu = {
      x: point.x,
      y: point.y,
      items: [
        { label: 'Свернуть', action: () => void win?.minimize() },
        { label: 'Развернуть или восстановить', action: () => void win?.toggleMaximize() },
        {
          label: pinned ? 'Не держать поверх остальных' : 'Поверх всех окон',
          action: () => void settings.patch({ ui: { alwaysOnTop: !pinned } }),
        },
        { label: mini ? 'Обычный режим' : 'Компактный режим', action: () => setMini(!mini) },
        { separator: true },
        { label: 'Закрыть', action: () => void win?.close() },
      ],
    };
  }

  function currentTrackMenu(event: MouseEvent) {
    event.preventDefault();
    const path = player.track?.path;
    if (!path) return;
    menu = {
      x: event.clientX,
      y: event.clientY,
      items: [
        { label: 'Открыть папку с треком', action: () => void api.revealInFolder(path) },
        { label: 'Во весь экран', action: () => (fullView = true) },
        { separator: true },
        {
          label: 'Найти обложку в интернете',
          disabled: path.startsWith('http'),
          action: () => void covers.fetchTrack(path),
        },
      ],
    };
  }

  // onMount с async не умеет возвращать функцию очистки, поэтому держим её сами.
  let unlistenDrop: UnlistenFn | null = null;

  onMount(async () => {
    try {
      info = await api.appInfo();
    } catch (err) {
      bootError = toErrorPayload(err);
    }
    await settings.load();
    view = settings.current?.ui.view ?? 'playlist';
    await player.init();
    await playlist.init();
    await library.init(settings.current?.library.folders ?? []);
    await gain.init();
    await lyricsScan.init();
    await sleep.init();
    await covers.init();

    try {
      unlistenDrop = await subscribeDragDrop();
    } catch (err) {
      console.warn('[ringloft] перетаскивание файлов недоступно', err);
    }
  });

  /** Вынесено в функцию, чтобы onMount не падал целиком, если подписка не удалась. */
  function subscribeDragDrop() {
    return getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === 'over' || event.payload.type === 'enter') {
        dropActive = true;
      } else if (event.payload.type === 'drop') {
        dropActive = false;
        void addPaths(event.payload.paths);
      } else {
        dropActive = false;
      }
    });
  }

  onDestroy(() => unlistenDrop?.());

  $effect(() => {
    const theme = settings.current?.ui.theme;
    if (theme) applyTheme(theme);
  });

  // Акцент из обложки — поверх своего: свой остаётся запасным для треков
  // без обложки и с бесцветной.
  $effect(() => {
    const ui = settings.current?.ui;
    const tint = ui?.accentFromCover ? player.coverTint : null;
    const fromCover = tint ? accentFromTint(tint, resolvedTheme(ui?.theme ?? 'dark')) : null;
    applyAccent(fromCover ?? ui?.accent);
  });

  $effect(() => {
    const ui = settings.current?.ui;
    if (ui) applyTypography(ui.fontSize, ui.font, ui.lyricsFont);
  });

  const mini = $derived(settings.current?.ui.mini ?? false);
  /** Что уже применено к окну; `null` — ещё ни разу. */
  let appliedMini: boolean | null = null;

  $effect(() => {
    const value = mini;
    if (appliedMini === value) return;
    const first = appliedMini === null;
    appliedMini = value;
    // На старте в обычном режиме окно уже нужного размера — его восстановил
    // плагин состояния, и трогать его нельзя: иначе каждый запуск сбрасывал
    // бы размер к стандартному.
    if (first && !value) return;
    void applyMiniWindow(value);
  });

  // Закрепление применяем всегда: оно живёт и в обычном режиме, просто
  // кнопка на него есть только в компактном.
  /** Что уже применено; `null` — ещё ни разу. */
  let appliedPin: boolean | null = null;

  $effect(() => {
    const wanted = settings.current?.ui.alwaysOnTop ?? false;
    if (appliedPin === wanted) return;
    // На старте без закрепления делать нечего: окно и так обычное, а на
    // Wayland это лишняя просьба к композитору на каждый запуск.
    if (appliedPin === null && !wanted) {
      appliedPin = wanted;
      return;
    }
    appliedPin = wanted;
    void applyAlwaysOnTop(wanted).then((done) => {
      // Если рабочий стол отказал, настройка не должна врать: снимаем её.
      if (wanted && !done) {
        console.warn('[ringloft] рабочий стол не даёт держать окно поверх остальных');
        void settings.patch({ ui: { alwaysOnTop: false } });
      }
    });
  });

  function setMini(value: boolean) {
    // Панели и полный экран в окне 430×84 не поместятся — закрываем их.
    if (value) {
      panel = 'none';
      fullView = false;
      menu = null;
    }
    void settings.patch({ ui: { mini: value } });
  }

  /** Папки бэкенд разворачивает сам, поэтому сюда можно кидать что угодно. */
  async function addPaths(paths: string[]) {
    const startIndex = playlist.count;
    const added = await playlist.add(paths);
    if (added > 0 && player.status === 'idle') await playlist.play(startIndex);
  }

  async function openFiles() {
    const selected = await open({
      multiple: true,
      directory: false,
      filters: [{ name: 'Аудио', extensions: AUDIO_EXTENSIONS }],
    });
    if (Array.isArray(selected)) await addPaths(selected);
    else if (typeof selected === 'string') await addPaths([selected]);
  }

  async function addFolder() {
    const selected = await open({ multiple: false, directory: true });
    if (typeof selected === 'string') await addPaths([selected]);
  }

  /** В полях ввода клавиши принадлежат полю, а не плееру. */
  function isTyping(target: EventTarget | null): boolean {
    const element = target as HTMLElement | null;
    if (!element) return false;
    return (
      element.tagName === 'INPUT' || element.tagName === 'TEXTAREA' || element.isContentEditable
    );
  }

  /**
   * Клавиатура в окне плеера. Медиаклавиши здесь нужны отдельно от системных:
   * на Wayland рабочий стол может не отдать их приложению глобально, но в
   * сфокусированном окне они приходят обычным событием.
   */
  /** Медиаклавиши: то же, что делают системные, но событием в окне. */
  const MEDIA_KEYS: Record<string, () => void> = {
    MediaPlayPause: () => void player.toggle(),
    MediaPlay: () => void player.toggle(),
    MediaPause: () => void player.toggle(),
    MediaTrackNext: () => void player.next(),
    MediaTrackPrevious: () => void player.prev(),
    MediaStop: () => void player.stop(),
  };

  /**
   * Действия горячих клавиш. `Record` по идентификаторам нарочно: забытое
   * сочетание не скомпилируется, и шпаргалка не разойдётся с делом.
   */
  const ACTIONS: Record<HotkeyId, (event: KeyboardEvent) => void> = {
    toggle: () => void player.toggle(),
    stop: () => void player.stop(),
    next: () => void player.next(),
    prev: () => void player.prev(),
    seekForward: () => void player.nudgePosition(5_000),
    seekForwardFar: () => void player.nudgePosition(30_000),
    seekBack: () => void player.nudgePosition(-5_000),
    seekBackFar: () => void player.nudgePosition(-30_000),
    volumeUp: () => void player.nudgeVolume(0.05),
    volumeDown: () => void player.nudgeVolume(-0.05),
    mute: () => void player.toggleMute(),
    shuffle: () => void settings.toggleShuffle(),
    repeat: () => void settings.cycleRepeat(),
    fullView: () => (fullView = !fullView),
    upnext: () => togglePanel('upnext'),
    lyrics: () => togglePanel('lyrics'),
    equalizer: () => togglePanel('eq'),
    settings: () => togglePanel('settings'),
    mini: () => setMini(!mini),
    view: () => switchView(view === 'playlist' ? 'library' : 'playlist'),
    help: () => togglePanel('keys'),
    fontBigger: () => void settings.nudgeFontSize(1),
    fontSmaller: () => void settings.nudgeFontSize(-1),
    fontReset: () => void settings.resetFontSize(),
    search: () => {
      switchView('playlist');
      playlist.focusSearch();
    },
    openFiles: () => void openFiles(),
    addFolder: () => void addFolder(),
    newTab: () => void playlist.createTab(),
    closeTab: () => playlist.closeActiveTab(),
    nextTab: () => void playlist.stepTab(1),
    prevTab: () => void playlist.stepTab(-1),
    tabByNumber: (event) => void playlist.activateByNumber(Number(event.key)),
  };

  /**
   * Клавиатура в окне плеера. Медиаклавиши здесь нужны отдельно от системных:
   * на Wayland рабочий стол может не отдать их приложению глобально, но в
   * сфокусированном окне они приходят обычным событием.
   */
  function keyDown(event: KeyboardEvent) {
    if (isTyping(event.target)) return;

    const media = MEDIA_KEYS[event.key];
    if (media) {
      event.preventDefault();
      media();
      return;
    }

    const hotkey = findHotkey(event);
    if (!hotkey) return;
    // Пробел на кнопке — это нажатие кнопки, а не пауза.
    if (hotkey.id === 'toggle' && (event.target as HTMLElement | null)?.tagName === 'BUTTON') {
      return;
    }
    // В компактном режиме половина действий некуда показывать.
    if (mini && !MINI_ALLOWED.has(hotkey.id)) return;

    event.preventDefault();
    ACTIONS[hotkey.id](event);
  }

  /** В окне 430×84 работает только то, что не открывает панелей. */
  const MINI_ALLOWED = new Set<HotkeyId>([
    'toggle', 'stop', 'next', 'prev',
    'seekForward', 'seekForwardFar', 'seekBack', 'seekBackFar',
    'volumeUp', 'volumeDown', 'mute', 'shuffle', 'repeat', 'mini',
  ]);

  const error = $derived(bootError ?? player.error ?? settings.error);
</script>

<svelte:window onkeydown={keyDown} />

<ResizeEdges />

{#if dropActive}
  <div class="dropzone"><span>Отпусти — добавлю в плейлист</span></div>
{/if}

{#if fullView}
  <FullView onclose={() => (fullView = false)} />
{/if}

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

{#if panel === 'settings'}
  <Modal title="Настройки" width={820} onclose={() => (panel = 'none')}>
    <SettingsPanel />
  </Modal>
{/if}

<!-- Подтверждение закрытия вкладки живёт здесь, а не в списке: Ctrl+W
     работает и из библиотеки. -->
<CloseTabDialog />
<SourcesDialog />
<Notice />

{#if panel === 'keys'}
  <Modal title="Горячие клавиши" width={820} onclose={() => (panel = 'none')}>
    <Hotkeys />
  </Modal>
{/if}

{#if panel === 'eq'}
  <Modal title="Эквалайзер" width={720} onclose={() => (panel = 'none')}>
    <Equalizer />
    <HeadphoneEq />
  </Modal>
{/if}

{#if mini}
  <MiniBar
    oncontext={windowMenu}
    onexpand={() => setMini(false)}
    pinned={settings.current?.ui.alwaysOnTop ?? false}
    onpin={(value) => settings.patch({ ui: { alwaysOnTop: value } })}
  />
{:else}
<!-- Полоса заголовка и панель инструментов — это одно и то же: системной
     рамки у окна нет, окно таскают за пустые места этой полосы. -->
<!-- role="toolbar": полоса и есть панель инструментов, а обработчик правого
     клика без роли ругается проверкой доступности. -->
<header class="chrome" role="toolbar" aria-label="Панель плеера" tabindex="-1" data-tauri-drag-region oncontextmenu={windowMenu}>
  <span
    class="logo"
    data-tauri-drag-region
    title={info ? `Ringloft ${info.version}${info.debug ? ' · debug' : ''}` : 'Ringloft'}
  >
    Ringloft
  </span>
  <span class="spacer" data-tauri-drag-region></span>

  <div class="views">
    <button class:active={view === 'playlist'} onclick={() => switchView('playlist')}>
      Плейлист
    </button>
    <button class:active={view === 'library'} onclick={() => switchView('library')}>
      Библиотека
    </button>
  </div>

  <span class="divider"></span>

  <div class="tools">
    {#each PANELS as item (item.id)}
      <button
        class="tool"
        class:active={panel === item.id}
        title={item.label}
        aria-label={item.label}
        onclick={() => togglePanel(item.id)}
      >
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d={item.icon} /></svg>
      </button>
    {/each}
  </div>

  <button class="tool" title="Компактный режим" aria-label="Компактный режим" onclick={() => setMini(true)}>
    <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2.5 3.5h11a1 1 0 0 1 1 1v7a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1v-7a1 1 0 0 1 1-1M1.5 9.5h13" /></svg>
  </button>

  <span class="divider"></span>
  <WindowButtons />
</header>

<main>
  {#if error}
    <ErrorBar
      {error}
      onclose={() => {
        bootError = null;
        player.error = null;
      }}
    />
  {/if}

  <GainBar />
  <LyricsBar />
  <CoversBar />

  <!-- Настройки и эквалайзер — модальные окна, полкой над списком они
       мешали больше, чем помогали. -->
  {#if panel === 'upnext' || panel === 'lyrics'}
    <div class="panel-slot">
      {#if panel === 'upnext'}
        <div class="card sheet"><UpNext /></div>
      {:else if panel === 'lyrics'}
        <div class="card sheet"><Lyrics /></div>
      {/if}
    </div>
  {/if}

  <div class="workspace">
    {#if view === 'library'}
      <div class="library-card">
        <LibraryPanel />
        <LibraryBrowser />
      </div>
    {:else}
      <PlaylistView
        onopenfiles={openFiles}
        onaddfolder={addFolder}
        onhotkeys={() => togglePanel('keys')}
      />
    {/if}
  </div>
</main>

<PlayerBar onexpand={() => (fullView = true)} oncontext={currentTrackMenu} />
{/if}

<style>
  .chrome {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 6px 0 12px;
    height: var(--chrome-h);
    background: var(--bg-elev);
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  .spacer {
    flex: 1;
    align-self: stretch;
  }
  .logo {
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--accent);
    margin-right: 6px;
  }
  /* Разделы — сегментированный переключатель по центру, отдельно от кнопок
     панелей справа: это разные по смыслу вещи. */
  .views {
    display: flex;
    gap: 2px;
    padding: 2px;
    border-radius: calc(var(--radius) + 2px);
    background: var(--bg-inset);
    border: 1px solid var(--border);
  }
  .views button {
    font: inherit;
    color: var(--fg-dim);
    background: none;
    border: none;
    border-radius: var(--radius);
    padding: 3px 14px;
    cursor: pointer;
    transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
  }
  .views button:hover {
    color: var(--fg);
  }
  /* Плейлист — основной режим, и переключатель не должен перетягивать
     внимание на себя: активная вкладка приподнята подложкой, а не залита
     акцентом. */
  .views button.active {
    background: var(--bg-elev);
    color: var(--fg);
    box-shadow: var(--shadow-card);
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .tool {
    width: 30px;
    height: 28px;
    display: grid;
    place-items: center;
    color: var(--fg-dim);
    background: none;
    border: none;
    border-radius: var(--radius);
    cursor: pointer;
    transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
  }
  .tool svg {
    width: 16px;
    height: 16px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.4;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .tool:hover:not(:disabled) {
    background: var(--row-hover);
    color: var(--fg);
  }
  .tool:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .tool.active {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .divider {
    width: 1px;
    height: 18px;
    margin: 0 4px;
    background: var(--border);
  }
  /* Панель появляется коротким выездом сверху: без этого она «прыгает»,
     и непонятно, что список сдвинулся именно из-за неё. */
  .panel-slot {
    flex: none;
    display: flex;
    flex-direction: column;
    animation: slide-in var(--dur-mid) var(--ease);
  }
  @keyframes slide-in {
    from {
      opacity: 0;
      transform: translateY(-6px);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .panel-slot {
      animation: none;
    }
  }
  main {
    flex: 1;
    min-height: 0;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .workspace {
    flex: 1;
    min-height: 0;
    display: flex;
    gap: 10px;
  }
  /* Библиотека — одна карточка с колонкой внутри, как плейлист. */
  .library-card {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .card {
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px 12px;
    box-shadow: var(--shadow-card);
    flex: none;
  }
  /* Панель не должна вытеснять плейлист: выше трети окна не растёт. */
  .sheet {
    max-height: 34vh;
    overflow: auto;
  }
  .dropzone {
    position: fixed;
    inset: 0;
    z-index: 30;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--accent) 16%, var(--bg));
    border: 2px dashed var(--accent);
    font-size: var(--fs-lg);
    color: var(--fg);
    pointer-events: none;
  }
</style>
