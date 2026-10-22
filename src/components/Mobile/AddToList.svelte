<script lang="ts">
  import { api } from '../../lib/api';
  import { plural } from '../../lib/format';
  import { notice } from '../../stores/notice.svelte.ts';
  import { playlist } from '../../stores/playlist.svelte.ts';
  import Sheet from './Sheet.svelte';
  import SheetItem from './SheetItem.svelte';

  /**
   * Куда положить треки из библиотеки: в собранный список или в новый.
   * Список, в который положили, становится открытым — туда и смотреть.
   *
   * Временной вкладки здесь нет: она пересобирается при каждом открытии
   * файлов, и положенное туда пропало бы.
   */
  const {
    paths,
    title,
    newName,
    onclose,
  }: {
    paths: string[];
    title: string;
    /** Название нового списка (у альбома — его название). */
    newName?: string;
    onclose: () => void;
  } = $props();

  const lists = $derived(playlist.tabs.filter((tab) => !tab.isTemporary));

  /**
   * Лист закрывается раньше, чем начнётся добавление, и вместе с ним уходят
   * его свойства — поэтому всё нужное берём в момент касания.
   */
  function pick(id: number | null) {
    const files = [...paths];
    const listName = newName;
    const existing = lists.find((tab) => tab.id === id)?.name;
    const target = existing
      ? `«${existing}»`
      : listName
        ? `новый список «${listName}»`
        : 'новый список';
    return () => void addTo(id, files, listName, target);
  }

  async function addTo(id: number | null, files: string[], listName: string | undefined, target: string) {
    // Новая вкладка сразу становится открытой — так её создаёт бэкенд.
    const tabId = id ?? (await api.playlistCreateTab(listName));
    await playlist.activateTab(tabId);
    const added = await playlist.add(files);
    notice.show(
      added > 0
        ? `В ${target} ${plural(added, 'добавлен', 'добавлено', 'добавлено')} ${added} ${plural(added, 'трек', 'трека', 'треков')}`
        : 'Ничего не добавилось',
    );
  }
</script>

<Sheet {title} {onclose}>
  {#snippet children(close)}
    {#each lists as tab (tab.id)}
      <SheetItem
        icon="playlist"
        label={tab.name}
        note={String(tab.count)}
        active={tab.isActive}
        onclick={() => close(pick(tab.id))}
      />
    {/each}
    <SheetItem icon="plus" label="Новый список" onclick={() => close(pick(null))} />
  {/snippet}
</Sheet>
