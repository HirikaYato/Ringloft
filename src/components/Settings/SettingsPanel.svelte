<script module lang="ts">
  let lastSection: 'sound' | 'window' | 'playlists' | 'look' | 'services' = 'sound';
</script>

<script lang="ts">
  import AcoustidSection from './AcoustidSection.svelte';
  import AppearanceSection from './AppearanceSection.svelte';
  import DiscordSection from './DiscordSection.svelte';
  import RateField from './RateField.svelte';
  import LastfmSection from './LastfmSection.svelte';
  import { api, type ReplayGainMode } from '../../lib/api';
  import { gain } from '../../stores/gain.svelte.ts';
  import { player } from '../../stores/player.svelte.ts';
  import { settings } from '../../stores/settings.svelte.ts';

  type Section = 'sound' | 'window' | 'playlists' | 'look' | 'services';
  const SECTIONS: { id: Section; label: string; icon: string }[] = [
    { id: 'sound', label: 'Звук', icon: 'M2.5 6h2.5l3.5-3v10L5 10H2.5zM11 5.5a3.5 3.5 0 0 1 0 5M12.8 3.5a6 6 0 0 1 0 9' },
    { id: 'window', label: 'Окно', icon: 'M2.5 3h11a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1M1.5 6h13' },
    { id: 'playlists', label: 'Плейлисты', icon: 'M2.5 4h8M2.5 7.5h8M2.5 11h5M13 3v7.5' },
    { id: 'look', label: 'Внешний вид', icon: 'M8 2a6 6 0 1 0 0 12c.8 0 1.2-.6 1.2-1.2 0-.9-.7-1.1-.7-1.9 0-.7.5-1.2 1.2-1.2H11a3 3 0 0 0 3-3C14 4.3 11.3 2 8 2M5 7.5h.01M6.5 4.8h.01M9.5 4.8h.01' },
    { id: 'services', label: 'Сервисы', icon: 'M6.5 9.5a3 3 0 0 0 4.2 0l2-2a3 3 0 0 0-4.2-4.2l-.6.6M9.5 6.5a3 3 0 0 0-4.2 0l-2 2a3 3 0 0 0 4.2 4.2l.6-.6' },
  ];
  /** Последний открытый раздел — на время работы плеера. */
  let section = $state<Section>(lastSection);
  $effect(() => {
    lastSection = section;
  });

  const REPLAY_GAIN: { id: ReplayGainMode; label: string }[] = [
    { id: 'off', label: 'выкл' },
    { id: 'track', label: 'по треку' },
    { id: 'album', label: 'по альбому' },
  ];

  const replayGain = $derived(settings.current?.audio.replayGain ?? 'off');
  /** Ход подсчёта виден и полосой над списком, но окно настроек её накрывает. */
  const gainStatus = $derived.by(() => {
    const progress = gain.progress;
    if (progress) {
      const phase = progress.phase === 'writing' ? 'Записываю метки' : 'Считаю громкость';
      return progress.total > 0
        ? `${phase}: ${progress.processed + 1} из ${progress.total}`
        : phase;
    }
    const report = gain.last;
    if (!report) return '';
    return `Посчитано: ${report.measured}, метка уже была у ${report.skipped}, не удалось ${report.failed}`;
  });
  const preamp = $derived(settings.current?.audio.replayGainPreampDb ?? 0);
  const crossfade = $derived(settings.current?.audio.crossfadeMs ?? 0);

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
</script>

<div class="settings">
  <nav aria-label="Разделы настроек">
    {#each SECTIONS as item (item.id)}
      <button class="section" class:active={section === item.id} onclick={() => (section = item.id)}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d={item.icon} /></svg>
        <span>{item.label}</span>
      </button>
    {/each}
  </nav>
  <div class="pane">
    {#if section === 'sound'}
<section class="settings-block">
  <h2>Звук</h2>

  <div class="field">
    <span class="label">Устройство вывода</span>
    <div class="control">
      <!-- Ключ по индексу намеренно: имена устройств от системы приходят
           какие есть, и повтор идентификатора не должен ронять всю панель. -->
      <select
        class="input"
        value={player.devices.find((device) => device.name === player.device)?.id ?? ''}
        onchange={(event) => player.setDevice(event.currentTarget.value || null)}
      >
        <option value="">Системное по умолчанию</option>
        {#each player.devices as device, index (index)}
          <option value={device.id}>{device.name} — {device.sampleRate / 1000} кГц</option>
        {/each}
      </select>
      <p class="hint">Сейчас открыто: {player.device ?? '—'}.</p>
    </div>
  </div>

  <RateField />

  <div class="field">
    <span class="label">Выравнивание громкости</span>
    <div class="control">
      <div class="row">
        {#each REPLAY_GAIN as mode (mode.id)}
          <button
            class="chip"
            class:active={replayGain === mode.id}
            onclick={() => setReplayGain(mode.id, preamp)}
          >
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
      <p class="hint">
        ReplayGain читается из тегов; без меток трек играет как есть. Свой
        подсчёт измеряет громкость по EBU R128 и записывает метку в файл.
      </p>
      <div class="row">
        {#if gain.running}
          <button class="ghost" onclick={() => gain.cancel()}>Остановить подсчёт</button>
        {:else}
          <button class="ghost" onclick={() => void gain.scanPlaylist()}>
            Посчитать для плейлиста
          </button>
          <button
            class="ghost"
            title="Пересчитать и те файлы, где метка уже есть"
            onclick={() => void gain.scanPlaylist(true)}
          >
            Пересчитать всё
          </button>
        {/if}
      </div>
      {#if gainStatus}
        <p class="hint">{gainStatus}</p>
      {:else if replayGain === 'off'}
        <p class="hint">Выравнивание выключено: метки запишутся, но громкость останется как есть.</p>
      {/if}
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
      <p class="hint">
        Ноль — следующий трек вступает ровно там, где кончился предыдущий. Иначе треки
        перекрываются с равномощным затуханием.
      </p>
    </div>
  </div>
</section>
    {:else if section === 'window'}
<section class="settings-block">
  <h2>Окно и уведомления</h2>

  <label class="check">
    <input
      type="checkbox"
      checked={settings.current?.ui.trackNotifications ?? true}
      onchange={(event) =>
        settings.patch({ ui: { trackNotifications: event.currentTarget.checked } })}
    />
    <span>показывать уведомление при смене трека</span>
  </label>

  <label class="check">
    <input
      type="checkbox"
      checked={settings.current?.ui.minimizeToTray ?? false}
      onchange={(event) => settings.patch({ ui: { minimizeToTray: event.currentTarget.checked } })}
    />
    <span>закрытие окна прячет плеер в трей</span>
  </label>

  <label class="check">
    <input
      type="checkbox"
      checked={settings.current?.ui.alwaysOnTop ?? false}
      onchange={(event) => settings.patch({ ui: { alwaysOnTop: event.currentTarget.checked } })}
    />
    <span>держать окно поверх остальных</span>
  </label>
  <div class="field">
    <span class="label">Горячие клавиши</span>
    <div class="control">
      <p class="hint">Список сочетаний — по F1 или из меню «⋯» на полосе вкладок.</p>
    </div>
  </div>
</section>
    {:else if section === 'playlists'}
<section class="settings-block">
  <h2>Плейлисты</h2>

  <label class="check">
    <input
      type="checkbox"
      checked={settings.current?.library.refreshPlaylists ?? true}
      onchange={(event) =>
        settings.patch({ library: { refreshPlaylists: event.currentTarget.checked } })}
    />
    <span>при запуске обновлять списки по их папкам</span>
  </label>
  <p class="hint">
    Папка, добавленная в список, запоминается как его источник: новые файлы из неё допишутся,
    пропавшие уйдут. Какие папки у списка — правый клик по вкладке → «Папки списка».
  </p>
</section>
    {:else if section === 'look'}
      <AppearanceSection />
    {:else}
      <LastfmSection />
      <DiscordSection />
      <AcoustidSection />
    {/if}
  </div>
</div>

<style>
  /* Разделы слева, содержимое справа: семь разделов одним свитком
     прокручивались бесконечно. */
  .settings {
    display: grid;
    grid-template-columns: minmax(150px, auto) minmax(0, 1fr);
    gap: 20px;
    min-height: min(560px, 70vh);
  }
  nav {
    position: sticky;
    top: 0;
    align-self: start;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-top: 8px;
  }
  .section {
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: var(--row-h);
    padding: 0 12px;
    font: inherit;
    font-size: var(--fs-sm);
    text-align: left;
    color: var(--fg-dim);
    background: none;
    border: 0;
    border-radius: 8px;
    cursor: pointer;
    white-space: nowrap;
  }
  .section:hover {
    color: var(--fg);
    background: var(--row-hover);
  }
  .section.active {
    color: var(--fg);
    background: var(--accent-soft);
  }
  .section.active::before {
    content: '';
    position: absolute;
    left: 0;
    top: 22%;
    bottom: 22%;
    width: 3px;
    border-radius: 2px;
    background: var(--accent);
  }
  .section svg {
    width: 16px;
    height: 16px;
    flex: none;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.4;
    stroke-linecap: round;
    stroke-linejoin: round;
    color: var(--fg-faint);
  }
  .section.active svg {
    color: var(--accent);
  }
  .pane {
    min-width: 0;
  }
  .slider {
    display: block;
    margin-top: 10px;
    font-size: var(--fs-xs);
    color: var(--fg-dim);
  }
  .slider input {
    display: block;
    width: 100%;
    max-width: 260px;
    margin-top: 4px;
  }
</style>
