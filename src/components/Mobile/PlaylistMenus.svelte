<script lang="ts" module>
  export type PlaylistSheet = 'none' | 'lists' | 'add' | 'more';
</script>

<script lang="ts">
  import { plural } from '../../lib/format';
  import { pickFiles, pickFolders } from '../../lib/pick';
  import { player } from '../../stores/player.svelte.ts';
  import { playlist } from '../../stores/playlist.svelte.ts';
  import Prompt from '../Common/Prompt.svelte';
  import Sheet from './Sheet.svelte';
  import SheetItem from './SheetItem.svelte';

  /**
   * Листы экрана плейлиста: выбор списка, добавление музыки, действия со
   * списком — и диалоги, которые из них открываются.
   */
  let { sheet = $bindable() }: { sheet: PlaylistSheet } = $props();

  const AUDIO = [
    'mp3', 'mp2', 'flac', 'wav', 'wave', 'ogg', 'oga', 'opus', 'weba', 'm4a', 'm4b', 'mp4', 'aac',
    'mka', 'aiff', 'aif',
  ];

  let prompt = $state<'none' | 'stream' | 'rename'>('none');
  const active = $derived(playlist.tabs.find((tab) => tab.isActive));

  async function add(paths: string[]) {
    if (paths.length === 0) return;
    const startIndex = playlist.count;
    const added = await playlist.add(paths);
    if (added > 0 && player.status === 'idle') await playlist.play(startIndex);
  }

  /** Адрес проверяем до добавления: иначе ошибка всплывёт только при игре. */
  function checkStreamUrl(value: string): string | null {
    let url: URL;
    try {
      url = new URL(value);
    } catch {
      return 'Это не похоже на адрес. Нужен полный, вида https://…';
    }
    return url.protocol === 'http:' || url.protocol === 'https:'
      ? null
      : 'Плеер умеет только http и https.';
  }
</script>

{#if sheet === 'lists'}
  <Sheet title="Плейлисты" onclose={() => (sheet = 'none')}>
    {#snippet children(close)}
      {#each playlist.tabs as tab (tab.id)}
        <SheetItem
          icon={tab.isTemporary ? 'note' : 'playlist'}
          label={tab.name}
          note={tab.isPlaying ? `играет · ${tab.count}` : String(tab.count)}
          active={tab.isActive}
          onclick={() => {
            void playlist.activateTab(tab.id);
            close();
          }}
        />
      {/each}
      <SheetItem
        icon="plus"
        label="Новый список"
        onclick={() => {
          void playlist.createTab();
          close();
        }}
      />
    {/snippet}
  </Sheet>
{/if}

{#if sheet === 'add'}
  <Sheet title="Добавить в список" onclose={() => (sheet = 'none')}>
    {#snippet children(close)}
      <SheetItem
        icon="folder"
        label="Папку"
        onclick={() => close(async () => add(await pickFolders({ title: 'Добавить папку' })))}
      />
      <SheetItem
        icon="note"
        label="Файлы"
        onclick={() =>
          close(async () =>
            add(
              await pickFiles({
                title: 'Добавить файлы',
                filter: { name: 'Аудио', extensions: AUDIO },
                multiple: true,
              }),
            ),
          )}
      />
      <SheetItem icon="radio" label="Интернет-радио" onclick={() => close(() => (prompt = 'stream'))} />
      <SheetItem
        icon="playlist"
        label="Открыть список (m3u, cue)"
        onclick={() =>
          close(async () => {
            const [list] = await pickFiles({
              title: 'Открыть список',
              filter: { name: 'Списки', extensions: ['m3u', 'm3u8', 'cue'] },
            });
            if (list) await playlist.importList(list);
          })}
      />
    {/snippet}
  </Sheet>
{/if}

{#if sheet === 'more' && active}
  {@const tab = active}
  <Sheet
    title="{tab.name} — {tab.count} {plural(tab.count, 'трек', 'трека', 'треков')}"
    onclose={() => (sheet = 'none')}
  >
    {#snippet children(close)}
      <SheetItem icon="edit" label="Переименовать" onclick={() => close(() => (prompt = 'rename'))} />
      <SheetItem
        icon="folder"
        label="Папки списка"
        note={tab.sources > 0 ? String(tab.sources) : ''}
        onclick={() => close(() => (playlist.sourcesFor = tab.id))}
      />
      {#if tab.sources > 0}
        <SheetItem
          icon="refresh"
          label="Обновить по папкам"
          onclick={() => {
            void playlist.refreshSources(tab.id);
            close();
          }}
        />
      {/if}
      <SheetItem
        icon="trash"
        label="Убрать пропавшие файлы"
        onclick={() => {
          void playlist.dropMissing();
          close();
        }}
      />
      <SheetItem
        icon="close"
        label="Очистить список"
        danger
        disabled={tab.count === 0}
        onclick={() => {
          void playlist.clear();
          close();
        }}
      />
      <SheetItem
        icon="close"
        label="Закрыть список"
        danger
        disabled={playlist.tabs.length < 2}
        onclick={() => close(() => playlist.requestClose(tab.id))}
      />
    {/snippet}
  </Sheet>
{/if}

{#if prompt === 'stream'}
  <Prompt
    title="Интернет-радио"
    label="Адрес потока"
    placeholder="https://example.org/stream"
    hint="Поток добавится строкой в список. Перемотки у радио нет, название приходит из эфира."
    validate={checkStreamUrl}
    onsubmit={(url) => void add([url])}
    onclose={() => (prompt = 'none')}
  />
{/if}

{#if prompt === 'rename' && active}
  {@const tab = active}
  <Prompt
    title="Переименовать список"
    label="Название"
    placeholder={tab.name}
    confirm="Переименовать"
    onsubmit={(name) => void playlist.renameTab(tab.id, name)}
    onclose={() => (prompt = 'none')}
  />
{/if}
