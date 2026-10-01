<script lang="ts">
  import Modal from '../Common/Modal.svelte';
  import type { SmartList, SmartRule, SmartRules } from '../../lib/api';

  let {
    list,
    onsave,
    ondelete,
    onclose,
  }: {
    list: SmartList;
    onsave: (list: SmartList) => void;
    ondelete: (id: number) => void;
    onclose: () => void;
  } = $props();

  const FIELDS = [
    { id: 'title', label: 'Название' },
    { id: 'artist', label: 'Исполнитель' },
    { id: 'album', label: 'Альбом' },
    { id: 'genre', label: 'Жанр' },
    { id: 'year', label: 'Год' },
    { id: 'rating', label: 'Оценка' },
    { id: 'playCount', label: 'Прослушиваний' },
    { id: 'lastPlayed', label: 'Слушали (момент)' },
    { id: 'addedAt', label: 'Добавлено (момент)' },
    { id: 'duration', label: 'Длительность, мс' },
    { id: 'folder', label: 'Папка' },
  ] as const;

  const OPS = [
    { id: 'contains', label: 'содержит' },
    { id: 'is', label: 'равно' },
    { id: 'isNot', label: 'не равно' },
    { id: 'greater', label: 'больше' },
    { id: 'less', label: 'меньше' },
    { id: 'empty', label: 'пусто' },
    { id: 'withinDays', label: 'за последние N дней' },
  ] as const;

  const SORTS = [
    { id: 'added', label: 'по добавлению' },
    { id: 'lastPlayed', label: 'по последнему прослушиванию' },
    { id: 'playCount', label: 'по числу прослушиваний' },
    { id: 'rating', label: 'по оценке' },
    { id: 'artist', label: 'по исполнителю' },
    { id: 'title', label: 'по названию' },
    { id: 'year', label: 'по году' },
    { id: 'random', label: 'случайно' },
  ] as const;

  // Правим копию: закрыли без сохранения — ничего не поменялось. Снимок
  // берётся один раз, дальше поля живут своей жизнью.
  const initial = snapshot();

  function snapshot(): SmartList {
    return $state.snapshot(list) as SmartList;
  }
  let name = $state(initial.name);
  let rules = $state<SmartRule[]>(initial.rules.rules.map((rule) => ({ ...rule })));
  let sort = $state<SmartRules['sort']>(initial.rules.sort);
  let descending = $state(initial.rules.descending);
  let limit = $state(initial.rules.limit === null ? '' : String(initial.rules.limit));

  function addRule() {
    rules = [...rules, { field: 'genre', op: 'is', value: '' }];
  }

  function save() {
    const parsed = Number.parseInt(limit, 10);
    onsave({
      id: list.id,
      name: name.trim() === '' ? 'Без названия' : name.trim(),
      rules: {
        rules,
        sort,
        descending,
        limit: Number.isFinite(parsed) && parsed > 0 ? parsed : null,
      },
    });
    onclose();
  }
</script>

<Modal title={initial.id === 0 ? 'Новый умный список' : 'Умный список'} width={640} {onclose}>
  <label class="row">
    <span>Название</span>
    <input class="input" type="text" bind:value={name} placeholder="Например, «под работу»" />
  </label>

  <h3>Условия</h3>
  <p class="hint">Должны совпасть все. Без условий список берёт всю библиотеку.</p>

  {#each rules as rule, index (index)}
    <div class="rule">
      <select class="input" bind:value={rule.field}>
        {#each FIELDS as field (field.id)}
          <option value={field.id}>{field.label}</option>
        {/each}
      </select>
      <select class="input" bind:value={rule.op}>
        {#each OPS as op (op.id)}
          <option value={op.id}>{op.label}</option>
        {/each}
      </select>
      <input
        class="input"
        type="text"
        bind:value={rule.value}
        disabled={rule.op === 'empty'}
        placeholder={rule.op === 'empty' ? '—' : 'значение'}
      />
      <button
        class="ghost"
        title="Убрать условие"
        onclick={() => (rules = rules.filter((_, at) => at !== index))}
      >
        ×
      </button>
    </div>
  {/each}

  <button class="ghost add" onclick={addRule}>+ условие</button>

  <h3>Порядок</h3>
  <div class="rule">
    <select class="input" bind:value={sort}>
      {#each SORTS as item (item.id)}
        <option value={item.id}>{item.label}</option>
      {/each}
    </select>
    <label class="check">
      <input type="checkbox" bind:checked={descending} disabled={sort === 'random'} />
      <span>в обратном порядке</span>
    </label>
    <input class="input" type="number" min="1" bind:value={limit} placeholder="сколько треков" />
  </div>

  <div class="actions">
    {#if initial.id > 0}
      <button class="ghost danger" onclick={() => { ondelete(initial.id); onclose(); }}>
        Удалить список
      </button>
    {/if}
    <span class="spacer"></span>
    <button class="ghost" onclick={onclose}>Отмена</button>
    <button class="primary" onclick={save}>Сохранить</button>
  </div>
</Modal>

<style>
  h3 {
    margin: 16px 0 4px;
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--fg-dim);
  }
  .row {
    display: grid;
    grid-template-columns: 110px minmax(0, 1fr);
    align-items: center;
    gap: 10px;
    margin-top: 8px;
  }
  .row span {
    color: var(--fg-dim);
    font-size: var(--fs-sm);
  }
  .rule {
    display: grid;
    grid-template-columns: minmax(0, 1.2fr) minmax(0, 1.2fr) minmax(0, 1fr) auto;
    gap: 6px;
    align-items: center;
    margin-bottom: 6px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-xs);
    color: var(--fg-dim);
    white-space: nowrap;
  }
  .add {
    margin-top: 2px;
    font-size: var(--fs-xs);
  }
  .hint {
    margin: 0 0 8px;
    font-size: var(--fs-xs);
    color: var(--fg-faint);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 18px;
  }
  .spacer {
    flex: 1;
  }
  .danger:hover {
    color: var(--danger);
    border-color: var(--danger);
  }
</style>
