<script lang="ts">
  /**
   * Частота без пересчёта. Сам плеер переоткрывает устройство на частоте
   * файла, но PipeWire по умолчанию разрешает одну частоту (48 кГц), и тогда
   * пересчёт неизбежен. Разрешить другие — настройка всей системы, поэтому
   * только по кнопке и с объяснением.
   */
  import { api, toErrorPayload, type RateStatus } from '../../lib/api';
  import { player } from '../../stores/player.svelte.ts';
  import { settings } from '../../stores/settings.svelte.ts';

  let status = $state<RateStatus | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);

  const khz = (rate: number) => `${(rate / 1000).toLocaleString('ru-RU')} кГц`;

  async function refresh() {
    try {
      status = await api.audioRateStatus();
    } catch (err) {
      error = toErrorPayload(err).message;
    }
  }

  // Трек сменился — частоты тоже могли.
  $effect(() => {
    void player.track?.path;
    void player.device;
    void refresh();
  });

  async function setExact(exact: boolean) {
    await api.playerSetExactRate(exact);
    await settings.load();
    await refresh();
  }

  async function allow(enabled: boolean) {
    busy = true;
    error = null;
    try {
      status = await api.pipewireAllowRates(enabled);
    } catch (err) {
      error = toErrorPayload(err).message;
    } finally {
      busy = false;
    }
  }

  const summary = $derived.by(() => {
    if (!status?.trackRate || !status.deviceRate) return 'Сейчас ничего не играет.';
    if (status.trackRate === status.deviceRate) return `Сейчас: ${khz(status.trackRate)} без пересчёта.`;
    return `Сейчас: файл ${khz(status.trackRate)}, устройство ${khz(status.deviceRate)} — звук пересчитывается.`;
  });
  /** PipeWire держит одну частоту — без разрешения пересчёт неизбежен. */
  const locked = $derived((status?.supported.length ?? 0) <= 1);
</script>

<div class="field">
  <span class="label">Частота</span>
  <div class="control">
    <label class="check">
      <input
        type="checkbox"
        checked={status?.exact ?? true}
        onchange={(event) => void setExact(event.currentTarget.checked)}
      />
      <span>играть на частоте файла, без пересчёта</span>
    </label>
    <p class="hint">{summary}</p>

    {#if status?.canManage}
      {#if locked && !status.managed}
        <p class="hint">
          PipeWire сейчас разрешает только {khz(status.supported[0] ?? 48000)}, поэтому всё остальное
          пересчитывается — в любом плеере. Кнопка ниже разрешит ему частоты 44,1–192 кГц: это
          настройка всей системы (файл в ~/.config/pipewire), её можно вернуть той же кнопкой.
        </p>
        <button class="ghost" disabled={busy} onclick={() => void allow(true)}>
          Разрешить PipeWire частоты файлов
        </button>
      {:else if status.managed}
        <p class="hint">Частоты PipeWire разрешены плеером: {status.supported.map(khz).join(', ')}.</p>
        <button class="ghost" disabled={busy} onclick={() => void allow(false)}>Вернуть как было</button>
      {/if}
    {/if}
    {#if error}<p class="hint error">{error}</p>{/if}
  </div>
</div>

<style>
  .error {
    color: var(--danger);
  }
</style>
