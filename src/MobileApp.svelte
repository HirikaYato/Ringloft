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
  import { device } from './stores/device.svelte.ts';
  import { player } from './stores/player.svelte.ts';
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

  useAppearance();

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

  const error = $derived(bootError ?? player.error ?? settings.error);
</script>

<div class="app">
  <main>
    <!-- Разделы прячем, а не убираем: у каждого своя прокрутка и открытая
         страница, и возвращаться надо туда, откуда ушли. -->
    <section hidden={tab !== 'playlist'}><PlaylistScreen shown={tab === 'playlist'} /></section>
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

  <MiniPlayer onopen={() => (nowOpen = true)} />
  <TabBar bind:tab />
</div>

{#if nowOpen}
  <NowPlaying onclose={() => (nowOpen = false)} />
{/if}

<CloseTabDialog />
<SourcesDialog />
<Notice />
<PathPicker />
<PermissionGate />

<style>
  .app {
    height: 100dvh;
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
</style>
