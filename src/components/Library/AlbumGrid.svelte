<script lang="ts">
  import { formatTime, plural } from '../../lib/format';
  import type { AlbumRow } from '../../lib/api';
  import { library } from '../../stores/library.svelte.ts';

  const {
    albums,
    onopen,
    onplay,
    oncontext,
  }: {
    albums: AlbumRow[];
    /** Правый клик — меню альбома. */
    oncontext?: (event: MouseEvent, album: AlbumRow) => void;
    /** Одиночный клик — показать треки альбома. */
    onopen: (album: AlbumRow) => void;
    /** Двойной — включить альбом целиком. */
    onplay: (album: AlbumRow) => void;
  } = $props();

  /**
   * Обложки грузим только у плиток, доехавших до экрана: на библиотеке в
   * тысячи альбомов вытаскивать их все сразу — это минуты работы впустую.
   */
  function lazyCover(node: HTMLElement, albumKey: string) {
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          void library.requestCover(albumKey);
          observer.disconnect();
        }
      },
      { rootMargin: '200px' },
    );
    observer.observe(node);
    return { destroy: () => observer.disconnect() };
  }
</script>

<div class="grid">
  {#each albums as album (album.key)}
    <button
      class="tile"
      onclick={() => onopen(album)}
      ondblclick={() => onplay(album)}
      oncontextmenu={(event) => oncontext?.(event, album)}
      title={album.title}
    >
      <div class="cover" use:lazyCover={album.key}>
        {#if library.covers.get(album.key)}
          <img src={library.covers.get(album.key)} alt="" loading="lazy" />
        {:else}
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 18V6l10-2v12" /><circle cx="6.5" cy="18" r="2.5" /><circle cx="16.5" cy="16" r="2.5" /></svg>
        {/if}
      </div>
      <span class="title">{album.title}</span>
      <span class="sub">
        {album.artist ?? '—'}{album.year ? ` · ${album.year}` : ''}
      </span>
      <span class="sub dim">{album.trackCount} {plural(album.trackCount, 'трек', 'трека', 'треков')} · {formatTime(album.durationMs)}</span>
    </button>
  {/each}
</div>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 12px;
    padding: 12px;
    overflow-y: auto;
    align-content: start;
  }
  .tile {
    display: flex;
    flex-direction: column;
    gap: 2px;
    background: none;
    border: 0;
    padding: 0;
    text-align: left;
    color: var(--fg);
    font: inherit;
    cursor: default;
  }
  .cover {
    aspect-ratio: 1;
    width: 100%;
    display: grid;
    place-items: center;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
    margin-bottom: 4px;
    transition: border-color var(--dur-fast) var(--ease);
  }
  .tile:hover .cover {
    border-color: var(--accent);
  }
  .cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .cover svg {
    width: 32px;
    height: 32px;
    fill: none;
    stroke: var(--fg-faint);
    stroke-width: 1.6;
    stroke-linecap: round;
  }
  .title {
    font-size: var(--fs-sm);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub {
    font-size: var(--fs-xs);
    color: var(--fg-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub.dim {
    color: var(--fg-faint);
  }
</style>
