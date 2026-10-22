<script lang="ts">
  import { api, type LibraryTrack } from '../../lib/api';
  import { player } from '../../stores/player.svelte.ts';
  import VirtualList from '../Common/VirtualList.svelte';
  import AddToList from './AddToList.svelte';
  import TrackRow from './TrackRow.svelte';

  /**
   * Треки альбома, жанра, папки, умного списка или поиска. Касание играет
   * всю выборку временной очередью, начиная с выбранного трека: собранные
   * руками плейлисты библиотека не трогает. Положить трек в такой список —
   * долгим нажатием.
   */
  const {
    tracks,
    empty = 'Здесь пусто.',
    showArtist = true,
  }: { tracks: LibraryTrack[]; empty?: string; showArtist?: boolean } = $props();

  let adding = $state<LibraryTrack | null>(null);

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
        current={track.path === player.track?.path}
        onpress={() => play(index)}
        onhold={() => (adding = track)}
      />
    {/snippet}
  </VirtualList>
{/if}

{#if adding}
  <AddToList
    paths={[adding.path]}
    title="«{adding.title}» — в список"
    onclose={() => (adding = null)}
  />
{/if}

<style>
  .note {
    margin: 48px 28px;
    text-align: center;
    color: var(--fg-faint);
  }
</style>
