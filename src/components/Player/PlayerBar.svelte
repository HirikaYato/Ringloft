<script lang="ts">
  import { player } from '../../stores/player.svelte.ts';
  import Transport from './Transport.svelte';

  let { onexpand, oncontext }: { onexpand: () => void; oncontext: (event: MouseEvent) => void } =
    $props();

  const track = $derived(player.track);
  const tags = $derived(player.tags);
  const title = $derived(player.title || 'Ничего не играет');
  const artist = $derived(tags?.artist ?? tags?.albumArtist ?? null);
  const album = $derived(tags?.album ?? null);
  const specs = $derived(
    track
      ? [
          track.codec.toUpperCase(),
          `${(track.sampleRate / 1000).toFixed(1)} кГц`,
          track.channels === 1 ? 'моно' : track.channels === 2 ? 'стерео' : `${track.channels} кан.`,
          tags?.bitrateKbps ? `${tags.bitrateKbps} кбит/с` : null,
        ]
          .filter(Boolean)
          .join(' · ')
      : '',
  );
</script>

<div
  class="playerbar"
  style:--tint={player.coverTint ?? 'transparent'}
  class:tinted={player.coverTint !== null}
>
  <button
    class="now"
    title={track ? `${specs}\n${track.path}` : 'Ничего не загружено'}
    onclick={onexpand}
    oncontextmenu={oncontext}
  >
    <span class="cover">
      {#if player.coverUrl}
        <img src={player.coverUrl} alt="" />
      {:else}
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 18V6l10-2v12" /><circle cx="6.5" cy="18" r="2.5" /><circle cx="16.5" cy="16" r="2.5" /></svg>
      {/if}
      <span class="expand">
        <svg viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M2.5 6V2.5H6M10 2.5h3.5V6M13.5 10v3.5H10M6 13.5H2.5V10" /></svg>
      </span>
    </span>
    <span class="meta">
      <span class="title">{title}</span>
      <span class="sub">
        {#if artist}{artist}{/if}{#if artist && album} — {/if}{#if album}{album}{/if}
        {#if !artist && !album && track}<span class="faint">{specs}</span>{/if}
      </span>
    </span>
  </button>

  <Transport />
</div>

<style>
  .playerbar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    background: var(--bg-elev);
    border-top: 1px solid var(--border);
    transition: background var(--dur-mid) var(--ease);
  }
  /* Цвет обложки уходит в подложку еле заметным пятном слева — достаточно,
     чтобы панель «принадлежала» треку, и мало, чтобы мешать читать. */
  .playerbar.tinted {
    background:
      radial-gradient(120% 260% at 0% 50%, color-mix(in srgb, var(--tint) 22%, transparent), transparent 60%),
      var(--bg-elev);
  }
  .now {
    flex: 0 1 240px;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 2px;
    font: inherit;
    text-align: left;
    color: inherit;
    background: none;
    border: none;
    border-radius: var(--radius);
    cursor: pointer;
  }
  .now:hover .expand {
    opacity: 1;
  }
  .now:focus-visible {
    outline: none;
    box-shadow: var(--focus);
  }
  .cover {
    position: relative;
    width: 42px;
    height: 42px;
    flex: none;
    display: grid;
    place-items: center;
    overflow: hidden;
    border-radius: var(--radius);
    background: var(--bg-inset);
    border: 1px solid var(--border);
  }
  .cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .cover > svg {
    width: 18px;
    height: 18px;
    fill: none;
    stroke: var(--fg-faint);
    stroke-width: 1.6;
    stroke-linecap: round;
  }
  /* Подсказка «жми и раскроется на всё окно» — показываем только по наведению,
     чтобы не спорить с самой обложкой. */
  .expand {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: #fff;
    background: rgb(0 0 0 / 45%);
    opacity: 0;
    transition: opacity var(--dur-fast) var(--ease);
  }
  .expand svg {
    width: 15px;
    height: 15px;
  }
  .meta {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .title,
  .sub {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .title {
    font-weight: 500;
  }
  .sub {
    font-size: var(--fs-xs);
    color: var(--fg-dim);
  }
  .faint {
    color: var(--fg-faint);
    font-family: var(--font-mono);
  }
</style>
