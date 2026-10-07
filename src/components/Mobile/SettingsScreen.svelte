<script lang="ts">
  import { api, type AppInfo, type ReplayGainMode } from '../../lib/api';
  import { pickFolders } from '../../lib/pick';
  import { device } from '../../stores/device.svelte.ts';
  import { library } from '../../stores/library.svelte.ts';
  import { settings } from '../../stores/settings.svelte.ts';
  import AppearanceSection from '../Settings/AppearanceSection.svelte';
  import LastfmSection from '../Settings/LastfmSection.svelte';
  import Icon from './Icon.svelte';

  /**
   * Настройки одной прокручиваемой страницей. Разделы оформления и Last.fm —
   * те же компоненты, что на десктопе; свои здесь только те, где набор
   * отличается (нет устройств вывода и частот: звуком на телефоне правит
   * система).
   */
  const { info }: { info: AppInfo | null } = $props();

  const REPLAY_GAIN: { id: ReplayGainMode; label: string }[] = [
    { id: 'off', label: 'выкл' },
    { id: 'track', label: 'по треку' },
    { id: 'album', label: 'по альбому' },
  ];

  const replayGain = $derived(settings.current?.audio.replayGain ?? 'off');
  const preamp = $derived(settings.current?.audio.replayGainPreampDb ?? 0);
  const crossfade = $derived(settings.current?.audio.crossfadeMs ?? 0);
  const permissions = $derived(device.permissions);

  // Громкость и переход живут в движке, поэтому не патчим настройки напрямую,
  // а перечитываем их после команды — иначе фронт и бэкенд разъедутся.
  async function setCrossfade(ms: number) {
    await api.playerSetCrossfade(ms);
    await settings.load();
  }

  async function setReplayGain(mode: ReplayGainMode, preampDb: number) {
    await api.playerSetReplayGain(mode, preampDb);
    await settings.load();
  }

  async function addFolder() {
    const picked = await pickFolders({ title: 'Папка библиотеки' });
    if (picked.length > 0) await library.addFolders(picked);
  }
</script>

<div class="screen">
  <header><h1>Настройки</h1></header>

  <div class="body mobile-settings">
    <section class="settings-block">
      <h2>Звук</h2>

      <div class="field">
        <span class="label">Выравнивание громкости</span>
        <div class="control">
          <div class="row">
            {#each REPLAY_GAIN as mode (mode.id)}
              <button class="chip" class:active={replayGain === mode.id} onclick={() => setReplayGain(mode.id, preamp)}>
                {mode.label}
              </button>
            {/each}
          </div>
          <label class="slider">
            <span>предусиление {preamp > 0 ? '+' : ''}{preamp.toFixed(1)} дБ</span>
            <input
              type="range"
              min="-15"
              max="15"
              step="0.5"
              value={preamp}
              disabled={replayGain === 'off'}
              onchange={(event) => setReplayGain(replayGain, Number(event.currentTarget.value))}
            />
          </label>
          <p class="hint">ReplayGain читается из тегов; без меток трек играет как есть.</p>
        </div>
      </div>

      <div class="field">
        <span class="label">Переход между треками</span>
        <div class="control">
          <label class="slider">
            <span>
              {crossfade === 0 ? 'без паузы (gapless)' : `кроссфейд ${(crossfade / 1000).toFixed(1)} с`}
            </span>
            <input
              type="range"
              min="0"
              max="10000"
              step="500"
              value={crossfade}
              onchange={(event) => setCrossfade(Number(event.currentTarget.value))}
            />
          </label>
        </div>
      </div>
      <p class="hint">Эквалайзер и профиль наушников — на экране «Сейчас играет».</p>
    </section>

    <section class="settings-block">
      <h2>Библиотека</h2>

      {#each library.folders as folder (folder)}
        <div class="folder">
          <Icon name="folder" />
          <span>{folder}</span>
          <button class="key" aria-label="Убрать папку из библиотеки" onclick={() => library.removeFolder(folder)}>
            <Icon name="close" size={18} />
          </button>
        </div>
      {:else}
        <p class="hint">Папок пока нет: библиотека пуста.</p>
      {/each}

      <div class="row actions">
        <button class="ghost" onclick={addFolder}>Добавить папку</button>
        <button class="ghost" disabled={library.scanning || library.folders.length === 0} onclick={() => library.scan()}>
          {library.scanning ? 'Сканирую…' : 'Пересканировать'}
        </button>
      </div>

      <label class="check">
        <input
          type="checkbox"
          checked={settings.current?.library.refreshPlaylists ?? true}
          onchange={(event) => settings.patch({ library: { refreshPlaylists: event.currentTarget.checked } })}
        />
        <span>при запуске обновлять списки по их папкам</span>
      </label>
    </section>

    <AppearanceSection />
    <LastfmSection />

    <section class="settings-block">
      <h2>Разрешения</h2>

      <div class="grant">
        <span>
          Доступ к файлам
          <small>{permissions?.files ? 'разрешён' : 'нет — плеер не видит музыку'}</small>
        </span>
        {#if !permissions?.files}
          <button class="ghost" onclick={() => device.request('files')}>Разрешить</button>
        {/if}
      </div>
      <div class="grant">
        <span>
          Уведомление плеера
          <small>{permissions?.notifications ? 'разрешено' : 'нет — карточки в шторке не будет'}</small>
        </span>
        {#if !permissions?.notifications}
          <button class="ghost" onclick={() => device.request('notifications')}>Разрешить</button>
        {/if}
      </div>
    </section>

    <section class="settings-block">
      <h2>О программе</h2>
      <p class="about">
        Ringloft {info?.version ?? ''}{info?.debug ? ' · отладочная сборка' : ''}<br />
        Лицензия MIT.
      </p>
    </section>
  </div>
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
    padding: 0 16px;
    border-bottom: 1px solid var(--border);
  }
  h1 {
    margin: 0;
    font-size: var(--fs-lg);
    font-weight: 600;
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 4px 16px 24px;
  }
  /* На десктопе подпись стоит слева от поля; в ширину телефона так не
     помещается — ставим её над полем. Правило общее для переиспользованных
     разделов, поэтому через :global. */
  .mobile-settings :global(.settings-block .field) {
    grid-template-columns: minmax(0, 1fr);
    gap: 6px;
  }
  .mobile-settings :global(.settings-block .label) {
    padding-top: 0;
    color: var(--fg-dim);
  }
  .mobile-settings :global(.chip),
  .mobile-settings :global(.ghost),
  .mobile-settings :global(.primary) {
    min-height: 40px;
  }
  .mobile-settings :global(.input) {
    min-height: 44px;
  }
  .slider {
    display: block;
    margin-top: 10px;
    color: var(--fg-dim);
  }
  .slider input {
    display: block;
    width: 100%;
    height: 32px;
  }
  .folder {
    min-height: 48px;
    display: flex;
    align-items: center;
    gap: 12px;
    color: var(--fg-dim);
  }
  .folder span {
    flex: 1;
    min-width: 0;
    font-size: var(--fs-sm);
    overflow-wrap: anywhere;
  }
  .key {
    flex: none;
    width: 44px;
    height: 44px;
    display: grid;
    place-items: center;
    color: var(--fg-faint);
    background: none;
    border: none;
    border-radius: 50%;
  }
  .actions {
    margin: 8px 0 14px;
  }
  .grant {
    min-height: 56px;
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .grant span {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  small {
    font-size: var(--fs-sm);
    color: var(--fg-faint);
  }
  .about {
    margin: 0;
    line-height: 1.6;
    color: var(--fg-dim);
  }
</style>
