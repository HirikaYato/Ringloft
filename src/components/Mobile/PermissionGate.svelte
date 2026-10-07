<script lang="ts">
  import { api } from '../../lib/api';
  import { device } from '../../stores/device.svelte.ts';
  import { library } from '../../stores/library.svelte.ts';

  /**
   * Первый запуск на телефоне: без доступа к файлам плеер не увидит ни одного
   * трека, поэтому до него дальше не пускаем. Уведомления — вторым шагом и
   * по желанию: от них зависит только карточка плеера в шторке.
   *
   * Про уведомления спрашиваем один раз, сразу после файлов. Если доступ к
   * файлам уже есть, экран не появляется вовсе — возвращаться к отказу при
   * каждом запуске незачем.
   */
  let step = $state<'files' | 'notifications' | 'done'>('files');
  /** Экран уже показывали: значит, доступ к файлам выдали при нас. */
  let shown = $state(false);

  const permissions = $derived(device.permissions);
  const visible = $derived(
    device.mobile && permissions !== null && step !== 'done' && (!permissions.files || shown),
  );

  $effect(() => {
    if (!permissions) return;
    if (!permissions.files) {
      shown = true;
      step = 'files';
    } else if (shown && step === 'files') {
      // Вернулись из настроек с доступом: уведомления спрашиваем, только
      // если их ещё нет.
      step = permissions.notifications ? 'done' : 'notifications';
      // При запуске плеер диска не видел, и сверять было не с чем.
      void library.scan();
      void api.playlistRefreshSources();
    } else if (step === 'notifications' && permissions.notifications) {
      step = 'done';
    }
  });

  // Системный вопрос про уведомления окно не прячет, и событие видимости
  // не приходит — пока экран открыт, перечитываем состояние сами.
  $effect(() => {
    if (!visible) return;
    const timer = setInterval(() => void device.refresh(), 1000);
    return () => clearInterval(timer);
  });
</script>

{#if visible}
  <div class="gate" role="dialog" aria-modal="true" aria-label="Разрешения">
    {#if step === 'files'}
      <h1>Доступ к музыке</h1>
      <p>
        Ringloft играет файлы прямо из папок телефона и не копирует их к себе. Для этого ему нужен
        «Доступ ко всем файлам»: откроются настройки, включите там переключатель и вернитесь.
      </p>
      <p class="aside">Читает плеер только то, что вы сами добавите в библиотеку и плейлисты.</p>
      <button class="primary" onclick={() => device.request('files')}>Открыть настройки</button>
    {:else}
      <h1>Плеер в шторке</h1>
      <p>
        Чтобы управлять музыкой с экрана блокировки и из шторки, разрешите уведомление — оно одно, с
        названием трека и кнопками.
      </p>
      <button class="primary" onclick={() => device.request('notifications')}>Разрешить</button>
      <button class="ghost" onclick={() => (step = 'done')}>Не сейчас</button>
    {/if}
  </div>
{/if}

<style>
  .gate {
    position: fixed;
    inset: 0;
    /* Поверх всего, включая окно выбора папок. */
    z-index: 50;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 14px;
    padding: calc(env(safe-area-inset-top) + 24px) 28px calc(env(safe-area-inset-bottom) + 24px);
    background: var(--bg);
  }
  h1 {
    margin: 0;
    font-size: 24px;
    font-weight: 600;
  }
  p {
    margin: 0;
    line-height: 1.5;
    color: var(--fg-dim);
  }
  .aside {
    font-size: var(--fs-sm);
    color: var(--fg-faint);
  }
  button {
    min-height: 48px;
    font-size: var(--fs-md);
  }
  .primary {
    margin-top: 10px;
  }
</style>
