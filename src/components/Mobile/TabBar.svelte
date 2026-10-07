<script lang="ts" module>
  export type Tab = 'playlist' | 'library' | 'settings';
</script>

<script lang="ts">
  import Icon, { type IconName } from './Icon.svelte';

  /** Три раздела внизу экрана: под большой палец. */
  let { tab = $bindable() }: { tab: Tab } = $props();

  const TABS: { id: Tab; label: string; icon: IconName }[] = [
    { id: 'playlist', label: 'Плейлист', icon: 'playlist' },
    { id: 'library', label: 'Библиотека', icon: 'library' },
    { id: 'settings', label: 'Настройки', icon: 'settings' },
  ];
</script>

<nav aria-label="Разделы">
  {#each TABS as item (item.id)}
    <button class:active={tab === item.id} aria-current={tab === item.id} onclick={() => (tab = item.id)}>
      <Icon name={item.icon} />
      <span>{item.label}</span>
    </button>
  {/each}
</nav>

<style>
  nav {
    flex: none;
    display: flex;
    padding-bottom: env(safe-area-inset-bottom);
    background: var(--bg-elev);
    border-top: 1px solid var(--border);
  }
  button {
    flex: 1;
    min-height: 56px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    font: inherit;
    font-size: var(--fs-xs);
    color: var(--fg-faint);
    background: none;
    border: none;
  }
  button.active {
    color: var(--accent);
  }
</style>
