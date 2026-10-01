<script lang="ts">
  /**
   * Действия с самими файлами: в корзину и переименование по шаблону.
   * Оба диалога здесь вместе — у них общая строка итога и общее правило:
   * сначала показать, что произойдёт, и только потом делать.
   */
  import Modal from '../Common/Modal.svelte';
  import { api, type FileOpResult, type RenamePreview } from '../../lib/api';
  import { plural } from '../../lib/format';

  let {
    mode,
    paths,
    onclose,
  }: { mode: 'trash' | 'rename'; paths: string[]; onclose: () => void } = $props();

  const PLACEHOLDERS = '{artist} {title} {album} {albumartist} {track} {year} {genre} {filename}';

  let pattern = $state('{artist} - {title}');
  let preview = $state<RenamePreview[]>([]);
  let busy = $state(false);
  let result = $state<FileOpResult | null>(null);

  const names = $derived(paths.map((path) => path.split('/').pop() ?? path));
  const count = $derived(paths.length);

  // Предпросмотр считает бэкенд: имя собирается из тегов, а их читает он.
  $effect(() => {
    if (mode !== 'rename') return;
    const asked = pattern;
    void api
      .filesRenamePreview(paths, asked)
      .then((rows) => {
        if (asked === pattern) preview = rows;
      })
      .catch(() => (preview = []));
  });

  const willRename = $derived(preview.filter((row) => !row.problem).length);

  async function run(): Promise<void> {
    busy = true;
    try {
      result =
        mode === 'trash'
          ? await api.filesToTrash(paths)
          : await api.filesRename(paths, pattern);
      if (mode === 'rename') preview = await api.filesRenamePreview(paths, pattern).catch(() => []);
    } catch (err) {
      result = { done: 0, failed: count, message: String(err) };
    } finally {
      busy = false;
    }
  }

  function summary(done: FileOpResult): string {
    const word = mode === 'trash' ? 'удалено в корзину' : 'переименовано';
    const parts = [`${word} ${done.done}`];
    if (done.failed > 0) parts.push(`не удалось ${done.failed}`);
    return parts.join(', ');
  }
</script>

<Modal
  title={mode === 'trash' ? 'Удалить в корзину' : 'Переименовать файлы'}
  width={mode === 'trash' ? 520 : 720}
  {onclose}
>
  {#if mode === 'trash'}
    <p>
      {count} {plural(count, 'файл', 'файла', 'файлов')} уедут в корзину системы — оттуда их можно
      вернуть. Строки из плейлиста исчезнут.
    </p>
    <ul class="names">
      {#each names.slice(0, 12) as name (name)}
        <li>{name}</li>
      {/each}
      {#if names.length > 12}
        <li class="more">…и ещё {names.length - 12}</li>
      {/if}
    </ul>
  {:else}
    <label class="row">
      <span>Шаблон</span>
      <input class="input" bind:value={pattern} spellcheck="false" />
    </label>
    <p class="hint">
      Подстановки: <code>{PLACEHOLDERS}</code>. Расширение остаётся своим, запрещённые символы
      заменяются подчёркиванием, текст <code>.lrc</code> уезжает вместе с треком.
    </p>

    <div class="preview">
      {#each preview as row (row.from)}
        <div class="line" class:bad={!!row.problem}>
          <span class="from">{row.from}</span>
          <span class="arrow">→</span>
          <span class="to">{row.problem ?? row.to}</span>
        </div>
      {:else}
        <p class="hint">Нечего показать.</p>
      {/each}
      {#if count > preview.length}
        <p class="hint">…и ещё {count - preview.length} — их не показываю, но переименую.</p>
      {/if}
    </div>
  {/if}

  {#if result}
    <p class="result" class:bad={result.failed > 0}>
      {summary(result)}{result.message ? `. ${result.message}` : ''}
    </p>
  {/if}

  <div class="actions">
    <button class="ghost" onclick={onclose}>{result ? 'Закрыть' : 'Отмена'}</button>
    {#if !result}
      <button
        class="primary"
        class:danger={mode === 'trash'}
        disabled={busy || count === 0 || (mode === 'rename' && willRename === 0)}
        onclick={() => void run()}
      >
        {mode === 'trash' ? 'Удалить в корзину' : `Переименовать (${willRename})`}
      </button>
    {/if}
  </div>
</Modal>

<style>
  p {
    margin: 0 0 10px;
    font-size: var(--fs-sm);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 8px;
  }
  .row span {
    flex: 0 0 auto;
    color: var(--fg-dim);
    font-size: var(--fs-sm);
  }
  .row input {
    flex: 1 1 auto;
    min-width: 0;
  }
  .names,
  .preview {
    max-height: 240px;
    overflow-y: auto;
    margin: 0 0 12px;
    padding: 8px 10px;
    list-style: none;
    background: var(--bg-inset);
    border-radius: var(--radius);
    font-size: var(--fs-sm);
  }
  .names li {
    padding: 1px 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .more,
  .hint {
    color: var(--fg-faint);
    font-size: var(--fs-xs);
  }
  /* Было → стало: середина фиксирована, а имена жмутся сами. */
  .line {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    gap: 8px;
    align-items: baseline;
    padding: 1px 0;
  }
  .line span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .line .from {
    color: var(--fg-dim);
  }
  .line .arrow {
    color: var(--fg-faint);
  }
  .line.bad .to {
    color: var(--danger);
  }
  code {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
  }
  .result {
    margin: 0 0 10px;
    color: var(--fg-dim);
  }
  .result.bad {
    color: var(--danger);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  /* Удаление — единственная кнопка, которую стоит подкрасить опасным. */
  .primary.danger {
    background: var(--danger);
    border-color: var(--danger);
  }
</style>
