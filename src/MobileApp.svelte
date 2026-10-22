<script lang="ts">
  import { onMount } from 'svelte';

  import ErrorBar from './components/Common/ErrorBar.svelte';
  import Notice from './components/Common/Notice.svelte';
  import LibraryScreen from './components/Mobile/LibraryScreen.svelte';
  import MiniPlayer from './components/Mobile/MiniPlayer.svelte';
  import NowPlaying from './components/Mobile/NowPlaying.svelte';
  import PathPicker from './components/Mobile/PathPicker.svelte';
  import PermissionGate from './components/Mobile/PermissionGate.svelte';
  import PlaylistScreen from './components/Mobile/PlaylistScreen.svelte';
  import SettingsScreen from './components/Mobile/SettingsScreen.svelte';
  import TabBar, { type Tab } from './components/Mobile/TabBar.svelte';
  import CloseTabDialog from './components/Playlist/CloseTabDialog.svelte';
  import SourcesDialog from './components/Playlist/SourcesDialog.svelte';
  import { api, toErrorPayload, type AppInfo, type ErrorPayload } from './lib/api';
  import { useAppearance } from './lib/appearance.svelte.ts';
  import { listenBack } from './lib/back';
  import { initStores } from './lib/boot';
  import { keyboard, watchKeyboard } from './lib/keyboard.svelte.ts';
  import { device } from './stores/device.svelte.ts';
  import { player } from './stores/player.svelte.ts';
  import { playlist } from './stores/playlist.svelte.ts';
  import { resolvedTheme, settings } from './stores/settings.svelte.ts';

  /**
   * Корневой макет для телефона. Сторы и команды — те же, что у десктопа
   * (`App.svelte`); своя здесь только раскладка: три раздела внизу, над ними
   * мини-плеер, «сейчас играет» — поверх всего.
   */
  let info = $state<AppInfo | null>(null);
  let bootError = $state<ErrorPayload | null>(null);
  let tab = $state<Tab>('playlist');
  let nowOpen = $state(false);
  /** Просьба плейлисту встать на играющую строку (растёт с каждой просьбой). */
  let reveal = $state(0);

  useAppearance();
  watchKeyboard();

  onMount(async () => {
    try {
      info = await api.appInfo();
    } catch (err) {
      bootError = toErrorPayload(err);
    }
    await device.init(true);
    await listenBack();
    await initStores();
    // Громкость на телефоне крутят кнопками корпуса, своей ручки здесь нет:
    // приглушённая настройка отнимала бы запас громкости, и вернуть её нечем.
    if (player.volume < 1) await player.setVolume(1);
  });

  // Окно идёт под строку состояния, и система не знает, какая у нас тема:
  // на тёмной теме при светлой системе значки вышли бы тёмными на тёмном.
  $effect(() => {
    const theme = settings.current?.ui.theme;
    if (!theme) return;
    void api.systemBars(resolvedTheme(theme) === 'light').catch((err) => {
      console.warn('[ringloft] значки строки состояния не перекрасились', err);
    });
  });

  /** Из плеера во весь экран — к играющему треку в его списке. */
  async function revealPlaying() {
    tab = 'playlist';
    const source = playlist.tabs.find((item) => item.isPlaying);
    if (source) await playlist.activateTab(source.id);
    reveal += 1;
  }

  const error = $derived(bootError ?? player.error ?? settings.error);
</script>

<div class="app">
  <main>
    <!-- Разделы прячем, а не убираем: у каждого своя прокрутка и открытая
         страница, и возвращаться надо туда, откуда ушли. -->
    <section hidden={tab !== 'playlist'}><PlaylistScreen shown={tab === 'playlist'} {reveal} /></section>
    <section hidden={tab !== 'library'}><LibraryScreen shown={tab === 'library'} /></section>
    <section hidden={tab !== 'settings'}><SettingsScreen {info} /></section>
  </main>

  {#if error}
    <ErrorBar
      {error}
      onclose={() => {
        bootError = null;
        player.error = null;
        settings.error = null;
      }}
    />
  {/if}

  <!-- Пока открыта клавиатура, нижние панели прячутся: иначе от экрана над
       ней оставалась бы половина, и та — под панелями. -->
  {#if keyboard.inset === 0}
    <MiniPlayer onopen={() => (nowOpen = true)} />
    <TabBar bind:tab />
  {/if}
</div>

{#if nowOpen}
  <NowPlaying onclose={() => (nowOpen = false)} onreveal={() => void revealPlaying()} />
{/if}

<CloseTabDialog />
<SourcesDialog />
<Notice />
<PathPicker />
<PermissionGate />

<style>
  .app {
    /* Отступ сверху входит в высоту: иначе с клавиатурой низ на эти 40 px
       уезжал под неё (без клавиатуры его прятало сжатие flex). */
    box-sizing: border-box;
    height: calc(100dvh - var(--keyboard, 0px));
    display: flex;
    flex-direction: column;
    /* Окно идёт под системные панели — отступаем от строки состояния сами;
       от нижней отступает панель разделов. */
    padding-top: env(safe-area-inset-top);
    background: var(--bg);
  }
  main {
    flex: 1;
    min-height: 0;
    position: relative;
  }
  section {
    position: absolute;
    inset: 0;
  }
  section[hidden] {
    display: none;
  }
  /* Сообщения встают над мини-плеером и панелью разделов, а не на них. */
  :global(:root) {
    --notice-bottom: calc(env(safe-area-inset-bottom) + 132px);
  }
</style>
