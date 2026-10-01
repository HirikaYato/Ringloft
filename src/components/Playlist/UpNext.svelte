<script lang="ts">
  import { playlist } from '../../stores/playlist.svelte.ts';
  import { settings } from '../../stores/settings.svelte.ts';
  import RowList from './RowList.svelte';

  // Порядок зависит от перемешивания и повтора, поэтому перечитываем его и
  // при смене этих настроек, а не только при смене списка.
  $effect(() => {
    void settings.current?.playback.shuffle;
    void settings.current?.playback.repeat;
    void playlist.loadUpcoming();
  });

  const shuffle = $derived(settings.current?.playback.shuffle ?? false);
</script>

<div class="upnext">
  <div class="head">
    <h2>Дальше по порядку</h2>
    <span class="hint">
      {shuffle ? 'перемешивание включено' : 'по порядку списка'}
    </span>
  </div>
  <div class="body">
    <RowList
      rows={playlist.upcoming}
      ordinal="position"
      empty="Дальше ничего нет: это последний трек, а повтор выключен."
      onactivate={(row) => playlist.play(row.index)}
    />
  </div>
</div>

<style>
  .upnext {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 10px;
    margin-bottom: 6px;
  }
  h2 {
    margin: 0;
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--fg-dim);
    font-weight: 600;
  }
  .hint {
    font-size: var(--fs-xs);
    color: var(--fg-faint);
  }
  .body {
    min-height: 0;
    overflow: auto;
  }
</style>
