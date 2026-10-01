<script lang="ts">
  /** Подтверждение закрытия вкладки: вернуть её потом нечем. */
  import Modal from '../Common/Modal.svelte';
  import { plural } from '../../lib/format';
  import { playlist } from '../../stores/playlist.svelte.ts';

  const tab = $derived(playlist.closeRequest);
</script>

{#if tab}
  <Modal title="Закрыть вкладку" width={420} onclose={() => (playlist.closeRequest = null)}>
    <p>
      «{tab.name}» — {tab.count} {plural(tab.count, 'трек', 'трека', 'треков')}. Вкладку закрыть можно,
      а вернуть потом будет нечем: сами файлы останутся на месте, пропадёт только список.
    </p>
    <div class="actions">
      <button class="ghost" onclick={() => (playlist.closeRequest = null)}>Отмена</button>
      <button class="primary danger" onclick={() => void playlist.confirmClose()}>Закрыть</button>
    </div>
  </Modal>
{/if}

<style>
  p {
    margin: 0 0 14px;
    font-size: var(--fs-sm);
    line-height: 1.5;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .primary.danger {
    background: var(--danger);
    border-color: var(--danger);
  }
</style>
