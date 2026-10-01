<script lang="ts">
  import type { ErrorPayload } from '../../lib/api';

  const { error, onclose }: { error: ErrorPayload; onclose: () => void } = $props();
</script>

<div class="error">
  <strong>{error.code}</strong>
  <span>{error.message}</span>
  {#if error.detail}<span class="detail">{error.detail}</span>{/if}
  <button onclick={onclose} title="Скрыть">×</button>
</div>

<style>
  .error {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 8px 12px;
    background: color-mix(in srgb, var(--danger) 14%, var(--bg-elev));
    border: 1px solid var(--danger);
    border-radius: var(--radius);
    font-size: var(--fs-sm);
  }
  strong {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--danger);
  }
  .detail {
    color: var(--fg-faint);
    font-size: var(--fs-xs);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  button {
    margin-left: auto;
    border: 0;
    background: none;
    color: var(--fg-dim);
    font-size: 16px;
    line-height: 1;
    cursor: pointer;
  }
</style>
