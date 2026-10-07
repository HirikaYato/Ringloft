<script lang="ts">
  import { onDestroy, type Snippet } from 'svelte';

  import { pushBack } from '../../lib/back';

  /**
   * Лист снизу — вместо контекстного меню и модального окна: до нижней
   * половины экрана палец дотягивается, до верхнего угла — нет.
   *
   * Закрывается касанием мимо, кнопкой «назад» и своим содержимым — ему
   * отдаётся функция `close`. `onclose` зовётся ровно один раз, каким бы
   * путём лист ни закрыли. Пункт, который открывает следующий слой (другой
   * лист, окно ввода), передаёт это действие в `close(then)`: сначала уходит
   * этот лист, потом открывается следующий.
   */
  const {
    title = '',
    onclose,
    children,
  }: {
    title?: string;
    onclose: () => void;
    children: Snippet<[(then?: () => void) => void]>;
  } = $props();

  let gone = false;
  const dismiss = pushBack(() => {
    if (!gone) onclose();
  });

  function close(then?: () => void) {
    dismiss();
    then?.();
  }

  // Лист убрали с экрана, минуя «назад» (закрылся экран под ним): из стопки
  // слоёв его надо вынуть, а закрывать уже нечего.
  onDestroy(() => {
    gone = true;
    dismiss();
  });
</script>

<div class="scrim" role="presentation" onclick={() => close()}></div>

<div class="sheet" role="dialog" aria-modal="true" aria-label={title || 'Действия'}>
  <div class="grip" aria-hidden="true"></div>
  {#if title}<h2>{title}</h2>{/if}
  <div class="body">{@render children(close)}</div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 30;
    background: var(--scrim);
    animation: fade-in var(--dur-mid) var(--ease);
  }
  .sheet {
    position: fixed;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: 31;
    max-height: 82dvh;
    display: flex;
    flex-direction: column;
    padding-bottom: env(safe-area-inset-bottom);
    background: var(--bg-elev);
    border-top: 1px solid var(--border);
    border-radius: 18px 18px 0 0;
    animation: rise var(--dur-mid) var(--ease);
  }
  .grip {
    flex: none;
    width: 36px;
    height: 4px;
    margin: 8px auto 4px;
    border-radius: 2px;
    background: var(--border);
  }
  h2 {
    margin: 0;
    padding: 6px 20px 10px;
    font-size: var(--fs-lg);
    font-weight: 600;
  }
  .body {
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  @keyframes fade-in {
    from {
      opacity: 0;
    }
  }
  @keyframes rise {
    from {
      transform: translateY(24px);
      opacity: 0;
    }
  }
</style>
