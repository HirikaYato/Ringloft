<script lang="ts">
  import Modal from './Modal.svelte';

  /** Одно поле и две кнопки: чаще всего этого и надо. */
  let {
    title,
    label,
    placeholder = '',
    confirm = 'Добавить',
    hint = '',
    /** Проверка перед отправкой: строка с причиной — это отказ. */
    validate,
    onsubmit,
    onclose,
  }: {
    title: string;
    label: string;
    placeholder?: string;
    confirm?: string;
    hint?: string;
    validate?: (value: string) => string | null;
    onsubmit: (value: string) => void;
    onclose: () => void;
  } = $props();

  let value = $state('');
  let field: HTMLInputElement | null = $state(null);

  $effect(() => field?.focus());

  const problem = $derived(value.trim() === '' ? null : (validate?.(value.trim()) ?? null));

  function submit() {
    const trimmed = value.trim();
    if (trimmed === '' || problem) return;
    onsubmit(trimmed);
    onclose();
  }
</script>

<Modal {title} {onclose} width={480}>
  <label>
    <span>{label}</span>
    <input
      class="input"
      type="text"
      {placeholder}
      bind:this={field}
      bind:value
      onkeydown={(event) => {
        if (event.key === 'Enter') submit();
      }}
    />
  </label>
  {#if problem}
    <p class="problem">{problem}</p>
  {:else if hint}
    <p class="hint">{hint}</p>
  {/if}
  <div class="actions">
    <button class="ghost" onclick={onclose}>Отмена</button>
    <button class="primary" disabled={value.trim() === '' || problem !== null} onclick={submit}>
      {confirm}
    </button>
  </div>
</Modal>

<style>
  label {
    display: block;
    margin-top: 8px;
  }
  label span {
    display: block;
    margin-bottom: 4px;
    font-size: var(--fs-sm);
    color: var(--fg-dim);
  }
  .hint,
  .problem {
    margin: 8px 0 0;
    font-size: var(--fs-xs);
  }
  .hint {
    color: var(--fg-faint);
  }
  .problem {
    color: var(--danger);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 14px;
  }
</style>
