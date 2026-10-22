<script lang="ts">
  import { api, type AlbumRow } from '../../lib/api';
  import { plural } from '../../lib/format';
  import { pickFolders } from '../../lib/pick';
  import { library } from '../../stores/library.svelte.ts';
  import Icon, { type IconName } from './Icon.svelte';

  /**
   * Главная библиотеки: плитки разделов, недавно добавленные альбомы и
   * итог по коллекции. Пока папок нет — просьба показать, где музыка.
   */
  const {
    sections,
    onsection,
    onalbum,
  }: {
    sections: { id: string; label: string; icon: IconName }[];
    onsection: (id: string) => void;
    onalbum: (album: AlbumRow) => void;
  } = $props();

  // Недавние альбомы перечитываем после каждого скана: он мог принести новые.
  let recent = $state<AlbumRow[]>([]);
  $effect(() => {
    void library.lastScan;
    void library.stats?.tracks;
    void api
      .libraryRecentAlbums(12)
      .then((albums) => (recent = albums))
      .catch((err) => console.warn('[ringloft] недавние альбомы не пришли', err));
  });

  $effect(() => {
    for (const album of recent) void library.requestCover(album.key);
  });

  async function addFolder() {
    const picked = await pickFolders({ title: 'Папка библиотеки' });
    if (picked.length > 0) await library.addFolders(picked);
  }

  const stats = $derived(library.stats);
  const summary = $derived(
    stats && stats.tracks > 0
      ? `${stats.tracks} ${plural(stats.tracks, 'трек', 'трека', 'треков')} · ${hours(stats.totalDurationMs)}`
      : '',
  );

  /** Длительность всей библиотеки: счёт идёт на часы, секунды тут шум. */
  function hours(ms: number): string {
    const total = Math.round(ms / 3_600_000);
    return total > 0 ? `${total} ${plural(total, 'час', 'часа', 'часов')} музыки` : 'меньше часа музыки';
  }

  const SCAN_PHASES: Record<string, string> = {
    walking: 'обход папок',
    reading: 'чтение тегов',
    cleaning: 'очистка',
  };
</script>

<div class="home">
  {#if library.scanning}
    <p class="status">
      Сканирую{library.progress ? `: ${SCAN_PHASES[library.progress.phase] ?? ''}` : ''}…
    </p>
  {/if}

  {#if library.folders.length === 0}
    <div class="empty">
      <p>Покажите плееру, где лежит музыка: он прочитает теги и разложит её по альбомам, исполнителям и жанрам.</p>
      <button class="primary" onclick={addFolder}>Добавить папку</button>
    </div>
  {:else}
    <div class="tiles">
      {#each sections as section (section.id)}
        <button class="tile" onclick={() => onsection(section.id)}>
          <Icon name={section.icon} />
          <span>{section.label}</span>
        </button>
      {/each}
    </div>

    {#if recent.length > 0}
      <h2>Недавно добавленные</h2>
      <div class="strip">
        {#each recent as album (album.key)}
          <button class="recent" onclick={() => onalbum(album)}>
            <span class="art">
              {#if library.covers.get(album.key)}
                <img src={library.covers.get(album.key)} alt="" loading="lazy" />
              {:else}
                <Icon name="note" size={28} />
              {/if}
            </span>
            <span class="name">{album.title || 'Без названия'}</span>
            <span class="sub">{album.artist ?? '—'}</span>
          </button>
        {/each}
      </div>
    {/if}

    {#if summary}<p class="status">{summary}</p>{/if}
  {/if}
</div>

<style>
  .home {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 16px;
  }
  .tiles {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .tile {
    min-height: 60px;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0 14px;
    font: inherit;
    text-align: left;
    color: var(--fg);
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }
  .tile:active {
    background: var(--row-hover);
  }
  .tile :global(svg) {
    color: var(--accent);
  }
  h2 {
    margin: 24px 0 10px;
    font-size: var(--fs-md);
    font-weight: 600;
    color: var(--fg-dim);
  }
  /* Лента едет вбок и упирается в края экрана, а не в отступы страницы. */
  .strip {
    display: flex;
    gap: 12px;
    margin: 0 -16px;
    padding: 0 16px 4px;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .recent {
    flex: none;
    width: 124px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 0;
    font: inherit;
    text-align: left;
    color: var(--fg);
    background: none;
    border: none;
  }
  .art {
    width: 124px;
    height: 124px;
    display: grid;
    place-items: center;
    margin-bottom: 4px;
    overflow: hidden;
    border-radius: var(--radius);
    color: var(--fg-faint);
    background: var(--bg-inset);
  }
  .art img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .name,
  .sub {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .name {
    font-size: var(--fs-sm);
    font-weight: 600;
  }
  .sub {
    font-size: var(--fs-xs);
    color: var(--fg-faint);
  }
  .status {
    margin: 20px 0 0;
    font-size: var(--fs-sm);
    color: var(--fg-faint);
  }
  .empty {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 32px 8px;
    color: var(--fg-dim);
    line-height: 1.5;
  }
  .empty p {
    margin: 0;
  }
  .empty .primary {
    min-height: 48px;
  }
</style>
