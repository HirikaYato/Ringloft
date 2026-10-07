<script lang="ts">
  import { thumbs } from '../../stores/thumbs.svelte.ts';
  import Icon from './Icon.svelte';

  /**
   * Обложка трека в строке списка. Картинку просит сама — когда строка
   * появилась в DOM, то есть доехала до экрана: списки виртуальные.
   */
  const { path, size = 44 }: { path: string; size?: number } = $props();

  const stream = $derived(path.startsWith('http://') || path.startsWith('https://'));
  const url = $derived(stream ? null : thumbs.url(path));

  $effect(() => {
    if (!stream) thumbs.want(path);
  });
</script>

<span class="cover" style:--size="{size}px">
  {#if url}
    <img src={url} alt="" loading="lazy" decoding="async" />
  {:else}
    <Icon name={stream ? 'radio' : 'note'} size={Math.round(size * 0.45)} />
  {/if}
</span>

<style>
  .cover {
    flex: none;
    width: var(--size);
    height: var(--size);
    display: grid;
    place-items: center;
    overflow: hidden;
    border-radius: 6px;
    color: var(--fg-faint);
    background: var(--bg-inset);
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
</style>
