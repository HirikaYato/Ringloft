<script lang="ts">
  import { notice } from '../../stores/notice.svelte.ts';
</script>

<!-- Область `status` живёт всегда: экранный диктор читает изменения в ней,
     а появившийся с текстом элемент мог бы и пропустить. -->
<div class="anchor" role="status">
  {#if notice.text}
    <button class="notice" title="Скрыть" onclick={() => notice.hide()}>{notice.text}</button>
  {/if}
</div>

<style>
  /* Над нижней панелью и по центру: туда смотрят после щелчка по меню. */
  .notice {
    position: fixed;
    left: 50%;
    bottom: 96px;
    z-index: 60;
    max-width: min(560px, calc(100vw - 32px));
    transform: translateX(-50%);
    padding: 8px 16px;
    font: inherit;
    font-size: var(--fs-sm);
    color: var(--fg);
    background: var(--bg-elev);
    border: 1px solid var(--border);
    /* Длинный итог переносится на вторую строку — «таблетка» тогда
       выглядела бы раздутой. */
    border-radius: 18px;
    box-shadow: 0 8px 24px rgb(0 0 0 / 30%);
    cursor: pointer;
    animation: rise var(--dur-mid) var(--ease);
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translate(-50%, 8px);
    }
  }
</style>
