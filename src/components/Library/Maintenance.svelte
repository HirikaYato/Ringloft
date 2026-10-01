<script lang="ts">
  /**
   * Проверка коллекции: похожие треки и пропавшие файлы.
   *
   * Плеер только показывает найденное. Какая из копий лишняя — по тегам не
   * видно (концертная запись и студийная называются одинаково), поэтому
   * отмечает пользователь, и ничего не отмечено заранее.
   */
  import Modal from '../Common/Modal.svelte';
  import { api, type DuplicateGroup, type LibraryTrack } from '../../lib/api';
  import { formatTime, plural } from '../../lib/format';
  import { library } from '../../stores/library.svelte.ts';

  let { onclose }: { onclose: () => void } = $props();

  type Tab = 'duplicates' | 'missing';

  let tab = $state<Tab>('duplicates');
  let groups = $state<DuplicateGroup[] | null>(null);
  let gone = $state<LibraryTrack[] | null>(null);
  let picked = $state<Set<string>>(new Set());
  let busy = $state(false);
  let notice = $state('');

  // Каждая вкладка грузится один раз: и дубликаты, и пропавшие — это полный
  // проход по базе, а пропавшие ещё и по диску.
  $effect(() => {
    if (tab === 'duplicates' && groups === null) void load('duplicates');
    if (tab === 'missing' && gone === null) void load('missing');
  });

  async function load(which: Tab): Promise<void> {
    busy = true;
    notice = '';
    try {
      if (which === 'duplicates') groups = await api.libraryDuplicates();
      else gone = await api.libraryMissing();
    } catch (err) {
      notice = `Не получилось проверить: ${err}`;
      if (which === 'duplicates') groups = [];
      else gone = [];
    } finally {
      busy = false;
    }
  }

  /** Отметки относятся к показанному списку: при смене вкладки они сбрасываются,
      иначе кнопка предлагала бы убрать то, чего на экране не видно. */
  function switchTab(next: Tab): void {
    if (tab === next) return;
    tab = next;
    picked = new Set();
    notice = '';
  }

  function toggle(path: string): void {
    const next = new Set(picked);
    if (!next.delete(path)) next.add(path);
    picked = next;
  }

  function pickAll(paths: string[]): void {
    picked = new Set(paths.every((path) => picked.has(path)) ? [] : paths);
  }

  /** Во группе оставляем первую копию, отмечаем остальные — самый частый выбор. */
  function pickExtras(): void {
    const next = new Set<string>();
    for (const group of groups ?? []) {
      for (const track of group.tracks.slice(1)) next.add(track.path);
    }
    picked = next;
  }

  const chosen = $derived([...picked]);

  async function forget(): Promise<void> {
    busy = true;
    try {
      const removed = await api.libraryForget(chosen);
      notice = `Убрано из библиотеки: ${removed}`;
      await refresh();
    } catch (err) {
      notice = `Не получилось убрать: ${err}`;
    } finally {
      busy = false;
    }
  }

  /** Отмеченные копии — в корзину, и заодно из библиотеки. */
  async function trash(): Promise<void> {
    busy = true;
    try {
      const result = await api.filesToTrash(chosen);
      await api.libraryForget(chosen);
      notice = `В корзину убрано ${result.done}${
        result.failed > 0 ? `, не удалось ${result.failed}: ${result.message ?? ''}` : ''
      }`;
      await refresh();
    } catch (err) {
      notice = `Не получилось убрать: ${err}`;
    } finally {
      busy = false;
    }
  }

  async function refresh(): Promise<void> {
    picked = new Set();
    groups = null;
    gone = null;
    await load(tab);
    await library.refreshStats();
    void library.loadMode(library.mode);
  }
</script>

<Modal title="Проверить коллекцию" width={820} {onclose}>
  <div class="tabs">
    <button class="chip" class:active={tab === 'duplicates'} onclick={() => switchTab('duplicates')}>
      Похожие треки{groups ? ` (${groups.length})` : ''}
    </button>
    <button class="chip" class:active={tab === 'missing'} onclick={() => switchTab('missing')}>
      Пропавшие файлы{gone ? ` (${gone.length})` : ''}
    </button>
  </div>

  {#if tab === 'duplicates'}
    <p class="hint">
      Совпали исполнитель с названием (у файлов без тегов — имя файла). Отметь лишние копии сам:
      живой концерт и студийная запись называются одинаково.
    </p>
    <div class="list">
      {#each groups ?? [] as group (group.key)}
        <div class="group">
          <div class="head">
            <span class="key">{group.key}</span>
            <span class="count">{group.tracks.length} {plural(group.tracks.length, 'копия', 'копии', 'копий')}</span>
            <button class="ghost" onclick={() => pickAll(group.tracks.map((t) => t.path))}>
              Отметить все
            </button>
          </div>
          {#each group.tracks as track (track.path)}
            <label class="track">
              <input
                type="checkbox"
                checked={picked.has(track.path)}
                onchange={() => toggle(track.path)}
              />
              <span class="path" title={track.path}>{'\u200e' + track.path}</span>
              <span class="dim">{track.durationMs ? formatTime(track.durationMs) : ''}</span>
            </label>
          {/each}
        </div>
      {:else}
        <p class="hint">{busy ? 'Проверяю…' : 'Похожих треков не нашлось.'}</p>
      {/each}
    </div>
  {:else}
    <p class="hint">
      Эти файлы записаны в библиотеке, но на диске их больше нет. Скан сам убирает такие строки
      внутри своих папок; остальное — здесь.
    </p>
    <div class="list">
      {#each gone ?? [] as track (track.path)}
        <label class="track">
          <input
            type="checkbox"
            checked={picked.has(track.path)}
            onchange={() => toggle(track.path)}
          />
          <span class="path" title={track.path}>{'\u200e' + track.path}</span>
        </label>
      {:else}
        <p class="hint">{busy ? 'Проверяю…' : 'Все файлы на месте.'}</p>
      {/each}
    </div>
  {/if}

  {#if notice}<p class="notice">{notice}</p>{/if}

  <div class="actions">
    {#if tab === 'duplicates' && (groups?.length ?? 0) > 0}
      <button class="ghost" onclick={pickExtras}>Отметить все копии кроме первой</button>
    {:else if tab === 'missing' && (gone?.length ?? 0) > 0}
      <button class="ghost" onclick={() => pickAll((gone ?? []).map((track) => track.path))}>
        Отметить все
      </button>
    {/if}
    <span class="spacer"></span>
    <button class="ghost" onclick={onclose}>Закрыть</button>
    {#if tab === 'duplicates'}
      <button class="ghost" disabled={busy || chosen.length === 0} onclick={() => void trash()}>
        В корзину ({chosen.length})
      </button>
    {/if}
    <button class="primary" disabled={busy || chosen.length === 0} onclick={() => void forget()}>
      Убрать из библиотеки ({chosen.length})
    </button>
  </div>
</Modal>

<style>
  .tabs {
    display: flex;
    gap: 6px;
    margin-bottom: 10px;
  }
  .hint {
    margin: 0 0 10px;
    color: var(--fg-faint);
    font-size: var(--fs-xs);
    line-height: 1.5;
  }
  .list {
    max-height: 46vh;
    overflow-y: auto;
    padding: 6px 8px;
    background: var(--bg-inset);
    border-radius: var(--radius);
  }
  .group {
    padding: 4px 0 6px;
  }
  .group + .group {
    border-top: 1px solid var(--border);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 0 4px;
  }
  .key {
    font-size: var(--fs-sm);
    font-weight: 600;
  }
  .count,
  .dim {
    color: var(--fg-faint);
    font-size: var(--fs-xs);
  }
  .head .ghost {
    margin-left: auto;
    font-size: var(--fs-xs);
  }
  /* Путь — самое важное в строке: по нему и различают копии. Обрезаем его
     слева (там общее начало), для этого нужен rtl, а перед путём — U+200E:
     без него ведущий слэш уезжает в конец строки. */
  .track {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 8px;
    align-items: center;
    padding: 1px 0;
    font-size: var(--fs-sm);
    cursor: pointer;
  }
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }
  .notice {
    margin: 10px 0 0;
    color: var(--fg-dim);
    font-size: var(--fs-sm);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 12px;
  }
  .spacer {
    flex: 1 1 auto;
  }
</style>
