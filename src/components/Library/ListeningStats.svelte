<script lang="ts">
  /** Итоги прослушивания: сводка, график, топы и часы суток. */
  import { api, toErrorPayload, type ListeningStats, type StatsPeriod } from '../../lib/api';
  import { plural } from '../../lib/format';
  import { fillBuckets, hoursText } from '../../lib/stats';

  let { onplay }: { onplay: (path: string) => void } = $props();

  const PERIODS: { id: StatsPeriod; label: string }[] = [
    { id: 'week', label: 'Неделя' },
    { id: 'month', label: 'Месяц' },
    { id: 'year', label: 'Год' },
    { id: 'all', label: 'Всё время' },
  ];

  let period = $state<StatsPeriod>('month');
  let stats = $state<ListeningStats | null>(null);
  let error = $state<string | null>(null);

  $effect(() => {
    const wanted = period;
    api
      .libraryListening(wanted)
      .then((result) => {
        if (period === wanted) stats = result;
      })
      .catch((err) => (error = toErrorPayload(err).message));
  });

  const buckets = $derived(stats ? fillBuckets(period, stats.buckets) : []);
  const bucketMax = $derived(Math.max(1, ...buckets.map((bucket) => bucket.plays)));
  const hourMax = $derived(Math.max(1, ...(stats?.byHour ?? [0])));
</script>

<div class="stats">
  <div class="periods">
    {#each PERIODS as item (item.id)}
      <button class="chip" class:active={period === item.id} onclick={() => (period = item.id)}>
        {item.label}
      </button>
    {/each}
  </div>

  {#if error}
    <p class="empty">{error}</p>
  {:else if stats && stats.plays === 0}
    <p class="empty">
      За этот период пусто. Прослушивание засчитывается, когда трек дослушан до половины (или до
      четырёх минут) — так же, как для Last.fm.
    </p>
  {:else if stats}
    <div class="tiles">
      <div class="tile"><b>{stats.plays}</b><span>{plural(stats.plays, 'прослушивание', 'прослушивания', 'прослушиваний')}</span></div>
      <div class="tile"><b>{hoursText(stats.listenedMs)}</b><span>музыки</span></div>
      <div class="tile"><b>{stats.tracks}</b><span>{plural(stats.tracks, 'трек', 'трека', 'треков')}</span></div>
      <div class="tile"><b>{stats.artists}</b><span>{plural(stats.artists, 'исполнитель', 'исполнителя', 'исполнителей')}</span></div>
    </div>

    <section>
      <h3>{period === 'week' || period === 'month' ? 'По дням' : 'По месяцам'}</h3>
      <div class="chart">
        {#each buckets as bucket (bucket.label)}
          <div class="bar" title="{bucket.title}: {bucket.plays}">
            <span style:height="{(bucket.plays / bucketMax) * 100}%"></span>
          </div>
        {/each}
      </div>
      <div class="axis">
        <span>{buckets[0]?.short ?? ''}</span>
        <span>{buckets.at(-1)?.short ?? ''}</span>
      </div>
    </section>

    <div class="tops">
      {#each [{ title: 'Исполнители', items: stats.topArtists }, { title: 'Треки', items: stats.topTracks }, { title: 'Альбомы', items: stats.topAlbums }] as top (top.title)}
        <section>
          <h3>{top.title}</h3>
          <ol>
            {#each top.items as item, index (index)}
              <li>
                <button
                  class="entry"
                  disabled={!item.path}
                  title={item.path ? 'Включить' : undefined}
                  onclick={() => item.path && onplay(item.path)}
                >
                  <span class="rank">{index + 1}</span>
                  <span class="text">
                    <span class="name">{item.name}</span>
                    {#if item.detail}<span class="detail">{item.detail}</span>{/if}
                  </span>
                  <span class="count">{item.plays}</span>
                  <span class="meter" style:width="{(item.plays / (top.items[0]?.plays ?? 1)) * 100}%"></span>
                </button>
              </li>
            {/each}
          </ol>
        </section>
      {/each}
    </div>

    <section>
      <h3>Когда слушаешь</h3>
      <div class="chart hours">
        {#each stats.byHour as count, hour (hour)}
          <div class="bar" title="{hour}:00 — {count}"><span style:height="{(count / hourMax) * 100}%"></span></div>
        {/each}
      </div>
      <div class="axis"><span>0:00</span><span>12:00</span><span>23:00</span></div>
    </section>
  {/if}
</div>

<style>
  .stats {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 4px 4px 16px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .periods {
    display: flex;
    gap: 6px;
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
    gap: 10px;
  }
  .tile {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 12px 14px;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .tile b {
    font-size: calc(var(--fs-lg) * 1.5);
    font-weight: 650;
    color: var(--fg);
  }
  .tile span {
    font-size: var(--fs-sm);
    color: var(--fg-dim);
  }
  h3 {
    margin: 0 0 8px;
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--fg-faint);
  }
  .chart {
    display: flex;
    align-items: flex-end;
    gap: 3px;
    height: 120px;
  }
  .chart.hours {
    height: 70px;
  }
  .bar {
    flex: 1;
    height: 100%;
    display: flex;
    align-items: flex-end;
  }
  .bar span {
    width: 100%;
    min-height: 2px;
    border-radius: 3px 3px 0 0;
    background: color-mix(in srgb, var(--accent) 75%, transparent);
  }
  .bar:hover span {
    background: var(--accent);
  }
  .axis {
    display: flex;
    justify-content: space-between;
    margin-top: 4px;
    font-size: var(--fs-xs);
    color: var(--fg-faint);
  }
  .tops {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: 18px;
  }
  ol {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .entry {
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 6px 8px;
    font: inherit;
    text-align: left;
    color: var(--fg);
    background: none;
    border: 0;
    border-radius: var(--radius);
  }
  .entry:not(:disabled) {
    cursor: pointer;
  }
  .entry:not(:disabled):hover {
    background: var(--row-hover);
  }
  .rank {
    width: 1.4em;
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--fg-faint);
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .name,
  .detail {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .name {
    font-size: var(--fs-sm);
  }
  .detail {
    font-size: var(--fs-xs);
    color: var(--fg-dim);
  }
  .count {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--fg-dim);
  }
  /* Тонкая полоска под строкой — доля от первого места. */
  .meter {
    position: absolute;
    left: 8px;
    bottom: 1px;
    height: 2px;
    max-width: calc(100% - 16px);
    border-radius: 1px;
    background: color-mix(in srgb, var(--accent) 45%, transparent);
  }
  .empty {
    color: var(--fg-faint);
    font-size: var(--fs-sm);
  }
</style>
