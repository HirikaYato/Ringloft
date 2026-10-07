<script lang="ts">
  import Icon, { type IconName } from './Icon.svelte';

  /** Строка листа: значок, подпись и, если нужно, пояснение справа. */
  const {
    icon,
    label,
    note = '',
    active = false,
    danger = false,
    disabled = false,
    onclick,
  }: {
    icon?: IconName;
    label: string;
    note?: string;
    active?: boolean;
    danger?: boolean;
    disabled?: boolean;
    onclick: () => void;
  } = $props();
</script>

<button class="item" class:active class:danger {disabled} {onclick}>
  {#if icon}<Icon name={icon} />{/if}
  <span class="label">{label}</span>
  {#if note}<span class="note">{note}</span>{/if}
</button>

<style>
  .item {
    width: 100%;
    min-height: 52px;
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 8px 20px;
    font: inherit;
    text-align: left;
    color: var(--fg);
    background: none;
    border: none;
  }
  .item:active:not(:disabled) {
    background: var(--row-hover);
  }
  .item:disabled {
    opacity: 0.4;
  }
  .item :global(svg) {
    color: var(--fg-dim);
  }
  .label {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .note {
    flex: none;
    font-size: var(--fs-sm);
    color: var(--fg-faint);
  }
  .active,
  .active :global(svg) {
    color: var(--accent);
  }
  .danger,
  .danger :global(svg) {
    color: var(--danger);
  }
</style>
