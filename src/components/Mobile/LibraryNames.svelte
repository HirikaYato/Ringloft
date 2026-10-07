<script lang="ts">
  import type { NamedCount } from '../../lib/api';
  import { plural } from '../../lib/format';
  import VirtualList from '../Common/VirtualList.svelte';
  import Icon, { type IconName } from './Icon.svelte';

  /** Список исполнителей, жанров или папок: имя и сколько за ним музыки. */
  const {
    items,
    icon,
    folders = false,
    onopen,
  }: {
    items: NamedCount[];
    icon: IconName;
    /** У папок имя — полный путь: показываем его хвост, остальное мелко. */
    folders?: boolean;
    onopen: (item: NamedCount) => void;
  } = $props();

  function tail(path: string): string {
    return path.split('/').filter(Boolean).at(-1) ?? path;
  }

  function count(item: NamedCount): string {
    const tracks = `${item.tracks} ${plural(item.tracks, 'трек', 'трека', 'треков')}`;
    if (item.albums === 0) return tracks;
    return `${item.albums} ${plural(item.albums, 'альбом', 'альбома', 'альбомов')} · ${tracks}`;
  }
</script>

{#if items.length === 0}
  <p class="note">Здесь пусто.</p>
{:else}
  <VirtualList {items} rowHeight={60}>
    {#snippet row(item)}
      <button class="row" onclick={() => onopen(item)}>
        <Icon name={icon} />
        <span class="text">
          <span class="name">{folders ? tail(item.name) : item.name || 'Без названия'}</span>
          <span class="sub">{folders ? item.name : count(item)}</span>
        </span>
        <Icon name="forward" size={16} />
      </button>
    {/snippet}
  </VirtualList>
{/if}

<style>
  .row {
    width: 100%;
    height: 60px;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 0 16px;
    font: inherit;
    text-align: left;
    color: var(--fg);
    background: none;
    border: none;
  }
  .row:active {
    background: var(--row-hover);
  }
  .row :global(svg) {
    color: var(--fg-faint);
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .name,
  .sub {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sub {
    font-size: var(--fs-sm);
    color: var(--fg-faint);
  }
  .note {
    margin: 48px 28px;
    text-align: center;
    color: var(--fg-faint);
  }
</style>
