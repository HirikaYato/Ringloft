<script lang="ts">
  /**
   * Папки, из которых собран список. Обновление дописывает из них новые
   * файлы и убирает пропавшие; недоступная папка (диск не подключён) не
   * считается пустой. Правки применяются сразу — кнопки «Сохранить» нет.
   */
  import { open } from '@tauri-apps/plugin-dialog';

  import Modal from '../Common/Modal.svelte';
  import { api, type PlaylistSources } from '../../lib/api';
  import { playlist } from '../../stores/playlist.svelte.ts';

  const id = $derived(playlist.sourcesFor);
  const tab = $derived(playlist.tabs.find((item) => item.id === id));
  let data = $state<PlaylistSources | null>(null);

  $effect(() => {
    if (id !== null) void load(id);
  });

  async function load(target: number) {
    data = await api.playlistSources(target);
  }

  async function save(sources: string[]) {
    if (id === null) return;
    await api.playlistSetSources(id, sources);
    await load(id);
  }

  async function addFolder() {
    const selected = await open({ multiple: true, directory: true });
    const picked = Array.isArray(selected) ? selected : selected ? [selected] : [];
    if (picked.length > 0 && data) await save([...data.sources, ...picked]);
  }

  async function restore() {
    if (id === null) return;
    await api.playlistRestoreExcluded(id);
    await load(id);
  }

  function close() {
    playlist.sourcesFor = null;
    data = null;
  }
</script>

{#if id !== null}
  <Modal title="Папки списка «{tab?.name ?? ''}»" width={520} onclose={close}>
    <p class="hint">
      Список следит за этими папками: при запуске плеера и по кнопке обновления новые файлы
      дописываются в конец, а пропавшие с диска уходят. Папка добавляется сама, когда её кладут в
      список.
    </p>

    {#if data && data.sources.length > 0}
      <ul class="folders">
        {#each data.sources as folder (folder)}
          <li>
            <span class="path" title={folder}><bdi>{folder}</bdi></span>
            <button
              class="chip"
              title="Перестать следить. Треки останутся в списке."
              onclick={() => void save(data?.sources.filter((item) => item !== folder) ?? [])}
            >
              убрать
            </button>
          </li>
        {/each}
      </ul>
    {:else if data}
      <p class="empty">Список собран руками — папок у него нет.</p>
    {/if}

    {#if data && data.excluded > 0}
      <p class="hint">
        Убрано из списка руками: {data.excluded}. При обновлении такие файлы не возвращаются.
        <button class="link" onclick={() => void restore()}>Вернуть их</button>
      </p>
    {/if}

    <div class="actions">
      <button class="ghost" onclick={() => void addFolder()}>Добавить папку</button>
      <span class="spacer"></span>
      <button
        class="primary"
        disabled={!data || data.sources.length === 0 || playlist.refreshing}
        onclick={async () => {
          await playlist.refreshSources(id ?? undefined);
          if (id !== null) await load(id);
        }}
      >
        {playlist.refreshing ? 'Обновляю…' : 'Обновить сейчас'}
      </button>
    </div>
  </Modal>
{/if}

<style>
  .hint,
  .empty {
    margin: 0 0 12px;
    font-size: var(--fs-sm);
    line-height: 1.5;
    color: var(--fg-dim);
  }
  .folders {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0 0 12px;
    padding: 0;
    list-style: none;
  }
  .folders li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 6px 6px 12px;
    background: var(--bg-inset);
    border-radius: var(--radius);
  }
  /* Длинный путь режем с начала: важен конец — имя папки. */
  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
    font-size: var(--fs-sm);
  }
  .link {
    padding: 0;
    font: inherit;
    color: var(--accent);
    background: none;
    border: none;
    cursor: pointer;
    text-decoration: underline;
  }
  .actions {
    display: flex;
    gap: 8px;
  }
  .spacer {
    flex: 1;
  }
</style>
