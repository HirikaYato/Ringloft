<script lang="ts">
  import { api, type LibraryTrack } from '../../lib/api';
  import VirtualList from '../Common/VirtualList.svelte';
  import TrackRow from './TrackRow.svelte';

  /**
   * Треки альбома, жанра, папки, умного списка или поиска. Касание играет
   * всю выборку временной очередью, начиная с выбранного трека: собранные
   * руками плейлисты библиотека не трогает.
   */
  const {
    tracks,
    empty = 'Здесь пусто.',
    showArtist = true,
  }: { tracks: LibraryTrack[]; empty?: string; showArtist?: boolean } = $props();

  export function play(start = 0) {
    if (tracks.length === 0) return;
    void api.playlistPlayFiles(
      tracks.map((track) => track.path),
      start,
    );
  }
</script>

{#if tracks.length === 0}
  <p class="note">{empty}</p>
{:else}
  <VirtualList items={tracks} rowHeight={60}>
    {#snippet row(track, index)}
      <TrackRow
        path={track.path}
        title={track.title}
        subtitle={showArtist
          ? [track.artist, track.album].filter(Boolean).join(' — ')
          : (track.artist ?? '')}
        durationMs={track.durationMs}
        onpress={() => play(index)}
      />
    {/snippet}
  </VirtualList>
{/if}

<style>
  .note {
    margin: 48px 28px;
    text-align: center;
    color: var(--fg-faint);
  }
</style>
