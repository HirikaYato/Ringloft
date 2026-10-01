<script lang="ts">
  /**
   * Теги по звуку для выделенных треков. Файлы идут по очереди (AcoustID не
   * любит пачки): отпечаток → варианты. Записывает обычная правка тегов, и
   * только то, что отмечено: неизвестные поля не трогаются, а не стираются.
   */
  import { untrack } from 'svelte';

  import Modal from '../Common/Modal.svelte';
  import { api, toErrorPayload, type TagCandidate, type TagEdit } from '../../lib/api';
  import { playlist } from '../../stores/playlist.svelte.ts';

  let { paths, onclose }: { paths: string[]; onclose: () => void } = $props();

  type Entry = {
    path: string;
    state: 'wait' | 'busy' | 'done' | 'error';
    candidates: TagCandidate[];
    choice: number;
    apply: boolean;
    error?: string;
  };

  let entries = $state<Entry[]>([]);
  let withGenre = $state(true);
  let writing = $state(false);
  let report = $state<string | null>(null);
  let cancelled = false;

  const name = (path: string) => path.split(/[\\/]/).pop() ?? path;
  const percent = (score: number) => `${Math.round(score * 100)}%`;
  const describe = (candidate: TagCandidate) =>
    [
      `${candidate.title} — ${candidate.artist}`,
      candidate.album ? `«${candidate.album}»` : '',
      [candidate.year, candidate.kind].filter(Boolean).join(', '),
      percent(candidate.score),
    ]
      .filter(Boolean)
      .join(' · ');

  $effect(() => {
    const list = paths;
    // Очередь читает и меняет `entries` — вне отслеживания, иначе эффект
    // подписался бы на собственные изменения и запускал её по кругу.
    untrack(() => {
      entries = list.map((path) => ({ path, state: 'wait', candidates: [], choice: 0, apply: false }));
      cancelled = false;
      void run();
    });
    return () => (cancelled = true);
  });

  async function run() {
    for (const entry of entries) {
      if (cancelled) return;
      entry.state = 'busy';
      try {
        entry.candidates = await api.identifyTrack(entry.path);
        entry.state = 'done';
        // Уверенное совпадение отмечаем сразу, сомнительное — пусть решат.
        entry.apply = (entry.candidates[0]?.score ?? 0) >= 0.8;
      } catch (err) {
        entry.state = 'error';
        entry.error = toErrorPayload(err).message;
        // Нет ключа — дальше идти бессмысленно.
        if (entry.error.includes('ключ')) return;
      }
    }
  }

  const ready = $derived(entries.filter((entry) => entry.apply && entry.candidates[entry.choice]));
  const busy = $derived(entries.some((entry) => entry.state === 'busy' || entry.state === 'wait'));

  async function write() {
    writing = true;
    report = null;
    const genres = new Map<string, string | null>();
    let written = 0;
    let failed = 0;
    for (const entry of ready) {
      const candidate = entry.candidates[entry.choice];
      if (!candidate) continue;
      let genre: string | null = null;
      if (withGenre && candidate.releaseGroupId) {
        if (!genres.has(candidate.releaseGroupId)) {
          genres.set(candidate.releaseGroupId, await api.identifyGenre(candidate.releaseGroupId).catch(() => null));
        }
        genre = genres.get(candidate.releaseGroupId) ?? null;
      }
      // `null` — поле не трогать: чего база не знает, то в файле останется.
      const edit: TagEdit = {
        title: candidate.title,
        artist: candidate.artist,
        album: candidate.album,
        albumArtist: candidate.albumArtist,
        genre,
        comment: null,
        year: candidate.year,
        track: candidate.track,
        trackTotal: candidate.trackTotal,
        disk: candidate.disk,
      };
      try {
        const result = await api.tagsWrite([entry.path], edit);
        written += result.written;
        failed += result.failed;
      } catch {
        failed += 1;
      }
    }
    writing = false;
    report = failed > 0 ? `Записано: ${written}, не удалось: ${failed}` : `Записано: ${written}`;
    void playlist.refresh();
  }
</script>

<Modal title="Теги по звуку" width={760} {onclose}>
  <ul class="files">
    {#each entries as entry, index (entry.path)}
      <li>
        <label class="pick">
          <input
            type="checkbox"
            disabled={entry.candidates.length === 0}
            bind:checked={entries[index]!.apply}
          />
          <span class="file" title={entry.path}>{name(entry.path)}</span>
        </label>
        <div class="result">
          {#if entry.state === 'wait'}
            <span class="dim">ждёт</span>
          {:else if entry.state === 'busy'}
            <span class="dim">слушаю и ищу…</span>
          {:else if entry.state === 'error'}
            <span class="error">{entry.error}</span>
          {:else if entry.candidates.length === 0}
            <span class="dim">в базе не нашлось</span>
          {:else}
            <select class="input" bind:value={entries[index]!.choice}>
              {#each entry.candidates as candidate, nth (nth)}
                <option value={nth}>{describe(candidate)}</option>
              {/each}
            </select>
          {/if}
        </div>
      </li>
    {/each}
  </ul>

  <div class="actions">
    <label class="check">
      <input type="checkbox" bind:checked={withGenre} />
      <span>жанр из MusicBrainz</span>
    </label>
    <span class="grow"></span>
    {#if report}<span class="dim">{report}</span>{/if}
    <button class="ghost" onclick={onclose}>Закрыть</button>
    <button class="primary" disabled={ready.length === 0 || writing || busy} onclick={() => void write()}>
      {writing ? 'Записываю…' : `Записать теги (${ready.length})`}
    </button>
  </div>
</Modal>

<style>
  .files {
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-height: 55vh;
    overflow-y: auto;
    margin: 0 0 14px;
    padding: 0;
    list-style: none;
  }
  li {
    display: grid;
    grid-template-columns: minmax(0, 0.9fr) minmax(0, 1.6fr);
    align-items: center;
    gap: 10px;
  }
  .pick {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .file {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--fs-sm);
  }
  .result {
    min-width: 0;
    font-size: var(--fs-sm);
  }
  .dim {
    color: var(--fg-faint);
    font-size: var(--fs-xs);
  }
  .error {
    color: var(--danger);
    font-size: var(--fs-xs);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .grow {
    flex: 1;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-sm);
  }
</style>
