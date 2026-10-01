<script lang="ts">
  /** Тема, акцент и шрифты. Применяются сразу — эффектами в App.svelte. */
  import { onMount } from 'svelte';
  import { api } from '../../lib/api';
  import { BUILTIN_FONTS, FONT_SIZE } from '../../lib/typography.svelte.ts';
  import { settings } from '../../stores/settings.svelte.ts';

  /** Готовые акценты: свой цвет тут же рядом, но с кнопки попасть быстрее. */
  const ACCENTS = ['#4f8cff', '#a06bff', '#e8833f', '#2fb573', '#e5534b', '#21b6c7', '#ff5c9d'];

  const THEMES: { id: 'dark' | 'light' | 'system'; label: string }[] = [
    { id: 'dark', label: 'тёмная' },
    { id: 'light', label: 'светлая' },
    { id: 'system', label: 'как в системе' },
  ];

  const ui = $derived(settings.current?.ui);
  const accent = $derived(ui?.accent ?? null);
  const fontSize = $derived(ui?.fontSize ?? FONT_SIZE.default);

  /** Шрифты из системы; сохранённый, но уже удалённый — тоже в списке. */
  let installed = $state<string[]>([]);
  const systemFonts = $derived.by(() => {
    const builtin = new Set(BUILTIN_FONTS.map((font) => font.id));
    const extra = [ui?.font, ui?.lyricsFont].filter(
      (font): font is string => !!font && !builtin.has(font) && !installed.includes(font),
    );
    return [...new Set([...extra, ...installed])];
  });

  onMount(async () => {
    try {
      installed = await api.fontsInstalled();
    } catch {
      installed = [];
    }
  });

  function setAccent(color: string | null) {
    void settings.patch({ ui: { accent: color } });
  }
</script>

{#snippet fontOptions()}
  {#each BUILTIN_FONTS as font (font.id)}
    <option value={font.id}>{font.label}</option>
  {/each}
  {#if systemFonts.length > 0}
    <optgroup label="Установленные в системе">
      {#each systemFonts as name (name)}
        <option value={name}>{name}</option>
      {/each}
    </optgroup>
  {/if}
{/snippet}

<section class="settings-block">
  <h2>Внешний вид</h2>

  <div class="field">
    <span class="label">Тема</span>
    <div class="control">
      <div class="row">
        {#each THEMES as theme (theme.id)}
          <button
            class="chip"
            class:active={ui?.theme === theme.id}
            onclick={() => settings.patch({ ui: { theme: theme.id } })}
          >
            {theme.label}
          </button>
        {/each}
      </div>
    </div>
  </div>

  <div class="field">
    <span class="label">Акцент</span>
    <div class="control">
      <div class="row">
        {#each ACCENTS as color (color)}
          <button
            class="swatch"
            class:active={accent === color}
            style:background={color}
            title={color}
            aria-label="Цвет {color}"
            onclick={() => setAccent(color)}
          ></button>
        {/each}
        <label class="swatch custom" title="Свой цвет">
          <input
            type="color"
            value={accent ?? '#4f8cff'}
            oninput={(event) => setAccent(event.currentTarget.value)}
          />
        </label>
        <button class="chip" class:active={accent === null} onclick={() => setAccent(null)}>
          как в теме
        </button>
      </div>
      <label class="check">
        <input
          type="checkbox"
          checked={ui?.accentFromCover ?? false}
          onchange={(event) =>
            settings.patch({ ui: { accentFromCover: event.currentTarget.checked } })}
        />
        <span>брать цвет из обложки играющего трека</span>
      </label>
      {#if ui?.accentFromCover}
        <p class="hint">Цвет выше остаётся для треков без обложки или с бесцветной.</p>
      {/if}
    </div>
  </div>

  <div class="field">
    <span class="label">Шрифт</span>
    <div class="control">
      <select
        class="input"
        value={ui?.font ?? 'system'}
        onchange={(event) => settings.patch({ ui: { font: event.currentTarget.value } })}
      >
        {@render fontOptions()}
      </select>
    </div>
  </div>

  <div class="field">
    <span class="label">Размер шрифта</span>
    <div class="control">
      <div class="row">
        <button
          class="chip"
          aria-label="Мельче"
          disabled={fontSize <= FONT_SIZE.min}
          onclick={() => void settings.nudgeFontSize(-1)}>A−</button
        >
        <span class="size">{fontSize} px</span>
        <button
          class="chip"
          aria-label="Крупнее"
          disabled={fontSize >= FONT_SIZE.max}
          onclick={() => void settings.nudgeFontSize(1)}>A+</button
        >
        <button
          class="chip"
          class:active={fontSize === FONT_SIZE.default}
          onclick={() => void settings.resetFontSize()}
        >
          по умолчанию
        </button>
      </div>
      <p class="hint">Ctrl+= и Ctrl+− делают то же из любого места, Ctrl+0 — сброс.</p>
    </div>
  </div>

  <div class="field">
    <span class="label">Текст песни</span>
    <div class="control">
      <select
        class="input"
        value={ui?.lyricsFont ?? 'manrope'}
        onchange={(event) => settings.patch({ ui: { lyricsFont: event.currentTarget.value } })}
      >
        {@render fontOptions()}
      </select>
      <p class="sample">Каждый вечер я гуляю по тихим улицам</p>
    </div>
  </div>
</section>

<style>
  .swatch {
    width: 24px;
    height: 24px;
    padding: 0;
    border-radius: 50%;
    border: 2px solid transparent;
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 25%);
    cursor: pointer;
  }
  .swatch.active {
    border-color: var(--fg);
  }
  /* Кружок «свой цвет» — это системный выбор цвета, спрятанный под тем же
     кружком: отдельная кнопка рядом выглядела бы лишней. */
  .swatch.custom {
    display: grid;
    place-items: center;
    overflow: hidden;
    background: conic-gradient(#e5534b, #e8833f, #2fb573, #21b6c7, #4f8cff, #a06bff, #e5534b);
  }
  .swatch.custom input {
    width: 40px;
    height: 40px;
    opacity: 0;
    cursor: pointer;
  }
  .size {
    min-width: 48px;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
  .sample {
    margin: 8px 0 0;
    font-family: var(--font-lyrics);
    font-size: calc(var(--fs-md) * 1.3);
    font-weight: 600;
    line-height: 1.35;
  }
</style>
