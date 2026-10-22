<script lang="ts" module>
  import { formatTime } from '../../lib/format';
  import { sleep } from '../../stores/sleep.svelte.ts';

  export type PlayerSheet = 'none' | 'sleep' | 'eq';

  /** Подпись кнопки и заголовок листа: что сейчас с таймером. */
  export function sleepLabel(): string {
    if (sleep.kind === 'timer') return `Пауза через ${formatTime(sleep.remainingMs)}`;
    if (sleep.kind === 'afterTrack') return 'Остановится после этого трека';
    return 'Таймер сна';
  }
</script>

<script lang="ts">
  import { player } from '../../stores/player.svelte.ts';
  import Equalizer from '../Player/Equalizer.svelte';
  import HeadphoneEq from '../Player/HeadphoneEq.svelte';
  import Sheet from './Sheet.svelte';
  import SheetItem from './SheetItem.svelte';

  /** Листы плеера во весь экран: таймер сна и эквалайзер. */
  let { sheet = $bindable() }: { sheet: PlayerSheet } = $props();

  const MINUTES = [15, 30, 45, 60, 90];

  function minutesLabel(minutes: number): string {
    if (minutes === 60) return 'Через час';
    if (minutes === 90) return 'Через полтора часа';
    return `Через ${minutes} минут`;
  }
</script>

{#if sheet === 'sleep'}
  <Sheet title={sleepLabel()} onclose={() => (sheet = 'none')}>
    {#snippet children(dismiss)}
      {#each MINUTES as minutes (minutes)}
        <SheetItem
          label={minutesLabel(minutes)}
          onclick={() => {
            void sleep.set({ kind: 'minutes', minutes });
            dismiss();
          }}
        />
      {/each}
      <!-- У радио конца нет — останавливаться не по чему. -->
      <SheetItem
        label="После этого трека"
        active={sleep.kind === 'afterTrack'}
        disabled={player.isStream}
        onclick={() => {
          void sleep.set({ kind: 'afterTrack' });
          dismiss();
        }}
      />
      {#if sleep.kind !== 'off'}
        <SheetItem
          icon="close"
          label="Выключить таймер"
          onclick={() => {
            void sleep.set({ kind: 'off' });
            dismiss();
          }}
        />
      {/if}
    {/snippet}
  </Sheet>
{/if}

{#if sheet === 'eq'}
  <Sheet title="Эквалайзер" onclose={() => (sheet = 'none')}>
    {#snippet children()}
      <div class="eq">
        <Equalizer />
        <HeadphoneEq />
      </div>
    {/snippet}
  </Sheet>
{/if}

<style>
  .eq {
    padding: 0 16px 16px;
  }
</style>
