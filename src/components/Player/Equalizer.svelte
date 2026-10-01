<script lang="ts">
  import { api } from '../../lib/api';
  import { settings } from '../../stores/settings.svelte.ts';

  /**
   * Подписи полос. Сами полосы задаёт Rust (`audio/dsp/eq.rs`: десять
   * ISO-октав), и менять их в одиночку нельзя — на десять полос рассчитаны и
   * пресеты ниже, и значение по умолчанию.
   */
  const LABELS = ['31', '62', '125', '250', '500', '1k', '2k', '4k', '8k', '16k'];

  const PRESETS: { name: string; bands: number[] }[] = [
    { name: 'Плоский', bands: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0] },
    { name: 'Рок', bands: [4, 3, -2, -3, -1, 2, 4, 5, 5, 4] },
    { name: 'Поп', bands: [-1, 2, 4, 4, 2, 0, -1, -1, 0, 1] },
    { name: 'Джаз', bands: [3, 2, 0, 1, -1, -1, 0, 1, 3, 4] },
    { name: 'Классика', bands: [4, 3, 2, 0, -1, -1, 0, 2, 3, 4] },
    { name: 'Больше баса', bands: [7, 6, 4, 2, 0, 0, 0, 0, 0, 0] },
    { name: 'Больше верха', bands: [0, 0, 0, 0, 0, 1, 3, 5, 6, 7] },
    { name: 'Голос', bands: [-2, -1, 0, 2, 4, 4, 3, 1, 0, -1] },
  ];

  const audio = $derived(settings.current?.audio);
  const enabled = $derived(audio?.eqEnabled ?? false);
  const preamp = $derived(audio?.eqPreampDb ?? 0);
  const bands = $derived(audio?.eqBandsDb ?? Array<number>(10).fill(0));

  /** Пока ползунок тащат, не долбим бэкенд на каждый пиксель. */
  let lastSentAt = 0;
  let pending: ReturnType<typeof setTimeout> | null = null;

  async function apply(next: { enabled?: boolean; preamp?: number; bands?: number[] }) {
    await api.playerSetEq(next.enabled ?? enabled, next.preamp ?? preamp, next.bands ?? bands);
    await settings.load();
  }

  function applyThrottled(next: { preamp?: number; bands?: number[] }) {
    const now = performance.now();
    if (pending) clearTimeout(pending);
    if (now - lastSentAt > 60) {
      lastSentAt = now;
      void apply(next);
    } else {
      pending = setTimeout(() => void apply(next), 60);
    }
  }

  function setBand(index: number, value: number) {
    const next = [...bands];
    next[index] = value;
    applyThrottled({ bands: next });
  }
</script>

<section class="eq">
  <div class="head">
    <label class="switch">
      <input
        type="checkbox"
        checked={enabled}
        onchange={(event) => apply({ enabled: event.currentTarget.checked })}
      />
      <span>Эквалайзер</span>
    </label>

    <select
      class="presets"
      disabled={!enabled}
      onchange={(event) => {
        const preset = PRESETS.find((item) => item.name === event.currentTarget.value);
        if (preset) void apply({ bands: [...preset.bands] });
        event.currentTarget.selectedIndex = 0;
      }}
    >
      <option value="">Пресет…</option>
      {#each PRESETS as preset (preset.name)}
        <option value={preset.name}>{preset.name}</option>
      {/each}
    </select>
  </div>

  <div class="bands" class:off={!enabled}>
    <div class="band preamp">
      <span class="value">{preamp > 0 ? '+' : ''}{preamp.toFixed(1)}</span>
      <input
        type="range"
        min="-15"
        max="15"
        step="0.5"
        value={preamp}
        disabled={!enabled}
        oninput={(event) => applyThrottled({ preamp: Number(event.currentTarget.value) })}
      />
      <span class="label">пред</span>
    </div>

    <div class="divider"></div>

    {#each LABELS as label, index (label)}
      <div class="band">
        <span class="value">{(bands[index] ?? 0) > 0 ? '+' : ''}{(bands[index] ?? 0).toFixed(1)}</span>
        <input
          type="range"
          min="-15"
          max="15"
          step="0.5"
          value={bands[index] ?? 0}
          disabled={!enabled}
          oninput={(event) => setBand(index, Number(event.currentTarget.value))}
        />
        <span class="label">{label}</span>
      </div>
    {/each}
  </div>
</section>

<style>
  /* Подложки и рамки нет: эквалайзер живёт внутри модального окна,
     фон рисует оно. */
  .eq {
    flex: none;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 8px;
  }
  .switch {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--fg-dim);
    font-weight: 600;
    cursor: pointer;
  }
  .switch input {
    accent-color: var(--accent);
  }
  .presets {
    font: inherit;
    font-size: var(--fs-xs);
    color: var(--fg);
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 3px 6px;
  }
  .bands {
    display: flex;
    gap: 10px;
    align-items: flex-end;
    transition: opacity var(--dur-fast) var(--ease);
  }
  .bands.off {
    opacity: 0.45;
  }
  .band {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    width: 34px;
  }
  .band input[type='range'] {
    /* Вертикальные ползунки: современный способ и запасной для WebKit. */
    writing-mode: vertical-lr;
    direction: rtl;
    -webkit-appearance: slider-vertical;
    appearance: slider-vertical;
    width: 18px;
    height: 110px;
    accent-color: var(--accent);
  }
  .value {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--fg-dim);
  }
  .label {
    font-size: var(--fs-xs);
    color: var(--fg-faint);
  }
  .preamp .label {
    color: var(--fg-dim);
  }
  .divider {
    width: 1px;
    align-self: stretch;
    background: var(--border);
    margin: 0 2px;
  }
</style>
