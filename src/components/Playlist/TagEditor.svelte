<script lang="ts">
  import Modal from '../Common/Modal.svelte';
  import { api, toErrorPayload, type TagEdit } from '../../lib/api';
  import { plural } from '../../lib/format';
  import { playlist } from '../../stores/playlist.svelte.ts';

  let { paths, onclose }: { paths: string[]; onclose: () => void } = $props();

  const TEXT = [
    { key: 'title', label: 'Название' },
    { key: 'artist', label: 'Исполнитель' },
    { key: 'album', label: 'Альбом' },
    { key: 'albumArtist', label: 'Исполнитель альбома' },
    { key: 'genre', label: 'Жанр' },
  ] as const;

  const NUMBERS = [
    { key: 'year', label: 'Год' },
    { key: 'track', label: 'Номер' },
    { key: 'trackTotal', label: 'Всего' },
    { key: 'disk', label: 'Диск' },
  ] as const;

  let values = $state<Record<string, string>>({});
  /** У выбранных файлов поле различается — показываем пустым с подсказкой. */
  let mixed = $state<Record<string, boolean>>({});
  /** Отправляем только то, что правили: остальное должно остаться как было. */
  let touched = $state(new Set<string>());
  let loading = $state(true);
  let saving = $state(false);
  let status = $state<string | null>(null);

  const KEYS = [...TEXT.map((field) => field.key), ...NUMBERS.map((field) => field.key), 'comment'];

  $effect(() => {
    void load();
  });

  async function load() {
    try {
      const tags = await Promise.all(paths.map((path) => api.tagsRead(path)));
      const next: Record<string, string> = {};
      const differs: Record<string, boolean> = {};
      for (const key of KEYS) {
        const seen = new Set(
          tags.map((item) => {
            const raw = (item as unknown as Record<string, unknown>)[key];
            return raw === null || raw === undefined ? '' : String(raw);
          }),
        );
        differs[key] = seen.size > 1;
        next[key] = (seen.size === 1 ? [...seen][0] : '') ?? '';
      }
      values = next;
      mixed = differs;
    } catch (err) {
      status = toErrorPayload(err).message;
    } finally {
      loading = false;
    }
  }

  function edited(key: string, value: string) {
    values[key] = value;
    touched.add(key);
    touched = new Set(touched);
  }

  function text(key: string): string | null {
    return touched.has(key) ? (values[key] ?? '') : null;
  }

  function number(key: string): number | null {
    if (!touched.has(key)) return null;
    const parsed = Number.parseInt(values[key] ?? '', 10);
    return Number.isFinite(parsed) && parsed > 0 ? parsed : 0;
  }

  async function save() {
    const edit: TagEdit = {
      title: text('title'),
      artist: text('artist'),
      album: text('album'),
      albumArtist: text('albumArtist'),
      genre: text('genre'),
      comment: text('comment'),
      year: number('year'),
      track: number('track'),
      trackTotal: number('trackTotal'),
      disk: number('disk'),
    };

    saving = true;
    status = null;
    try {
      const result = await api.tagsWrite(paths, edit);
      await playlist.refresh();
      if (result.failed === 0) {
        onclose();
        return;
      }
      status = `Записано ${result.written}, не удалось ${result.failed}: ${result.message ?? ''}`;
    } catch (err) {
      status = toErrorPayload(err).message;
    } finally {
      saving = false;
    }
  }
</script>

<Modal
  title={paths.length > 1
    ? `Теги: ${paths.length} ${plural(paths.length, 'файл', 'файла', 'файлов')}`
    : 'Теги'}
  {onclose}
>
  {#if loading}
    <p class="note">Читаю теги…</p>
  {:else}
    <div class="form">
      {#each TEXT as field (field.key)}
        <label>
          <span>{field.label}</span>
          <input
            class="input"
            type="text"
            value={values[field.key] ?? ''}
            placeholder={mixed[field.key] ? 'разные значения' : ''}
            oninput={(event) => edited(field.key, event.currentTarget.value)}
          />
        </label>
      {/each}

      <div class="row">
        <span></span>
        <div class="numbers">
          {#each NUMBERS as field (field.key)}
            <label class="num">
              <span>{field.label}</span>
              <input
                type="number"
                min="0"
                value={values[field.key] ?? ''}
                placeholder={mixed[field.key] ? '—' : ''}
                oninput={(event) => edited(field.key, event.currentTarget.value)}
              />
            </label>
          {/each}
        </div>
      </div>

      <label>
        <span>Комментарий</span>
        <textarea
          class="input"
          rows="2"
          value={values.comment ?? ''}
          placeholder={mixed.comment ? 'разные значения' : ''}
          oninput={(event) => edited('comment', event.currentTarget.value)}
        ></textarea>
      </label>
    </div>

    <p class="hint">
      Пустое поле стирает тег. Поля, которых не касались, остаются как были.
      {#if paths.length > 1}Правка применится ко всем выбранным файлам.{/if}
    </p>

    {#if status}<p class="status">{status}</p>{/if}

    <div class="actions">
      <button class="ghost" onclick={onclose}>Отмена</button>
      <button class="primary" disabled={saving || touched.size === 0} onclick={save}>
        {saving ? 'Записываю…' : 'Записать'}
      </button>
    </div>
  {/if}
</Modal>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 8px;
  }
  label,
  .row {
    display: grid;
    grid-template-columns: 150px minmax(0, 1fr);
    align-items: center;
    gap: 10px;
  }
  label span {
    color: var(--fg-dim);
    font-size: var(--fs-sm);
  }
  textarea {
    resize: vertical;
  }
  .numbers {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 8px;
  }
  .num {
    display: block;
  }
  .num span {
    display: block;
    margin-bottom: 3px;
  }
  .hint {
    margin: 12px 0 0;
    font-size: var(--fs-xs);
    color: var(--fg-faint);
  }
  .note {
    margin: 10px 0;
    color: var(--fg-dim);
  }
  .status {
    margin: 8px 0 0;
    font-size: var(--fs-sm);
    color: var(--danger);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 14px;
  }
</style>
