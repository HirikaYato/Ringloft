<script lang="ts">
  /**
   * Профиль наушников (AutoEQ): выравнивает АЧХ конкретной модели. Ищем по
   * базе AutoEQ (индекс кэшируется на две недели), выбранный профиль живёт в
   * настройках и работает без сети. Можно взять и свой ParametricEQ.txt.
   */
  import { open } from '@tauri-apps/plugin-dialog';

  import { api, toErrorPayload, type HeadphoneHit } from '../../lib/api';
  import { settings } from '../../stores/settings.svelte.ts';

  const audio = $derived(settings.current?.audio);
  const profile = $derived(audio?.headphone ?? null);
  const enabled = $derived(audio?.headphoneEnabled ?? false);

  let query = $state('');
  let hits = $state<HeadphoneHit[]>([]);
  let searching = $state(false);
  let applying = $state<string | null>(null);
  let error = $state<string | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;

  /** Индекс ищется локально, но первый раз качается — не на каждую букву. */
  function search(value: string) {
    query = value;
    clearTimeout(timer);
    if (value.trim().length < 2) {
      hits = [];
      return;
    }
    timer = setTimeout(async () => {
      searching = true;
      error = null;
      try {
        hits = await api.headphonesSearch(value);
      } catch (err) {
        error = toErrorPayload(err).message;
      } finally {
        searching = false;
      }
    }, 250);
  }

  async function adopt(action: () => Promise<unknown>, key: string) {
    applying = key;
    error = null;
    try {
      await action();
      await settings.load();
      hits = [];
      query = '';
    } catch (err) {
      error = toErrorPayload(err).message;
    } finally {
      applying = null;
    }
  }

  async function importFile() {
    const picked = await open({ multiple: false, filters: [{ name: 'AutoEQ', extensions: ['txt'] }] });
    if (typeof picked === 'string') await adopt(() => api.headphonesImport(picked), 'file');
  }
</script>

<section class="phones">
  <div class="head">
    <label class="switch">
      <input
        type="checkbox"
        checked={enabled}
        disabled={!profile}
        onchange={(event) =>
          adopt(() => api.headphonesSetEnabled(event.currentTarget.checked), 'toggle')}
      />
      <span>Наушники</span>
    </label>
    {#if profile}
      <span class="current" title="{profile.filters.length} фильтров, предусиление {profile.preampDb} дБ">
        {profile.name}<span class="dim"> · {profile.source}</span>
      </span>
    {:else}
      <span class="dim">профиль не выбран</span>
    {/if}
    <span class="grow"></span>
    <button class="ghost" disabled={applying !== null} onclick={() => void importFile()}>Из файла…</button>
  </div>

  <input
    class="input"
    type="search"
    placeholder="Модель наушников: HD 650, Moondrop Aria, AirPods Pro…"
    value={query}
    oninput={(event) => search(event.currentTarget.value)}
  />

  {#if searching}
    <p class="hint">Ищу в базе AutoEQ…</p>
  {:else if hits.length > 0}
    <ul class="hits">
      {#each hits as hit (hit.path)}
        <li>
          <button
            class="hit"
            disabled={applying !== null}
            onclick={() => void adopt(() => api.headphonesApply(hit), hit.path)}
          >
            <span class="name">{hit.name}</span>
            <span class="dim">{applying === hit.path ? 'загружаю…' : hit.source}</span>
          </button>
        </li>
      {/each}
    </ul>
  {:else if query.trim().length >= 2}
    <p class="hint">В базе такой модели нет. Можно сделать профиль на autoeq.app и взять его «из файла».</p>
  {:else}
    <p class="hint">
      Профиль выравнивает звук конкретной модели по её измерениям (база AutoEQ). Работает поверх
      эквалайзера выше; одна модель бывает измерена несколькими людьми — подойдёт любая строка.
    </p>
  {/if}
  {#if error}<p class="hint error">{error}</p>{/if}
</section>

<style>
  .phones {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 16px;
    padding-top: 14px;
    border-top: 1px solid var(--border);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }
  .switch {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--fg-dim);
    font-weight: 600;
    cursor: pointer;
  }
  .current {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--fs-sm);
  }
  .grow {
    flex: 1;
  }
  .dim {
    color: var(--fg-faint);
    font-size: var(--fs-xs);
  }
  .hits {
    max-height: 200px;
    overflow-y: auto;
    margin: 0;
    padding: 0;
    list-style: none;
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .hit {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    width: 100%;
    padding: 6px 10px;
    font: inherit;
    font-size: var(--fs-sm);
    text-align: left;
    color: var(--fg);
    background: none;
    border: 0;
    cursor: pointer;
  }
  .hit:hover:not(:disabled) {
    background: var(--row-hover);
  }
  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hint {
    margin: 0;
    font-size: var(--fs-xs);
    color: var(--fg-faint);
    line-height: 1.5;
  }
  .error {
    color: var(--danger);
  }
</style>
