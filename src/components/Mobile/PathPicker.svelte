<script lang="ts">
  import { api, type FolderEntry, type FolderListing } from '../../lib/api';
  import { pushBack } from '../../lib/back';
  import { picker } from '../../stores/picker.svelte.ts';

  /**
   * Своё окно выбора папки или файлов — на весь экран. Системное на телефоне
   * не годится (см. `lib/pick.ts`), а с «Доступом ко всем файлам» плеер
   * видит диск сам.
   *
   * Папку выбирают, стоя в ней: кнопка внизу берёт ту, что открыта. Так не
   * нужен отдельный жест «выбрать» поверх жеста «зайти».
   */
  const request = $derived(picker.request);

  let roots = $state<FolderEntry[]>([]);
  /** `null` — список хранилищ, выше которого подниматься некуда. */
  let listing = $state<FolderListing | null>(null);
  let chosen = $state<string[]>([]);
  let failure = $state<string | null>(null);
  let loading = $state(false);

  /** Что ответить просившему; пусто — отмена. */
  let answer: string[] = [];
  let dismiss: (() => void) | null = null;

  $effect(() => {
    if (!request) return;
    listing = null;
    chosen = [];
    failure = null;
    answer = [];
    // Окно — слой для кнопки «назад»: она его отменяет. Свои кнопки уходят
    // тем же путём — так просивший получает ответ ровно один раз.
    dismiss = pushBack(() => {
      dismiss = null;
      picker.finish(answer);
    });
    void start();
  });

  function finish(paths: string[]) {
    answer = paths;
    dismiss?.();
  }

  async function start() {
    try {
      roots = await api.storageRoots();
      // Хранилище одно почти всегда — сразу заходим в него.
      const [only, ...rest] = roots;
      if (only && rest.length === 0) await enter(only.path);
    } catch (err) {
      failure = message(err);
    }
  }

  async function enter(path: string) {
    loading = true;
    failure = null;
    try {
      listing = await api.storageList(path, request?.extensions ?? []);
      chosen = [];
    } catch (err) {
      failure = message(err);
    } finally {
      loading = false;
    }
  }

  function up() {
    if (!listing) return;
    // Из корня хранилища — к списку хранилищ, а не в системные папки над ним.
    if (roots.some((root) => root.path === listing?.path) || !listing.parent) {
      listing = null;
      return;
    }
    void enter(listing.parent);
  }

  function toggle(path: string) {
    if (!request?.multiple) {
      finish([path]);
      return;
    }
    chosen = chosen.includes(path) ? chosen.filter((item) => item !== path) : [...chosen, path];
  }

  function confirm() {
    if (request?.mode === 'folder') {
      if (listing) finish([listing.path]);
    } else {
      finish(chosen);
    }
  }

  function message(err: unknown): string {
    return typeof err === 'object' && err && 'message' in err ? String(err.message) : String(err);
  }

  /** Путь без корня хранилища: «/storage/emulated/0» человеку ничего не говорит. */
  const place = $derived.by(() => {
    if (!listing) return 'Хранилища';
    const root = roots.find((item) => listing?.path.startsWith(item.path));
    if (!root) return listing.path;
    const rest = listing.path.slice(root.path.length).replace(/^\//, '');
    return rest ? `${root.name} / ${rest.replaceAll('/', ' / ')}` : root.name;
  });

  const canGoUp = $derived(listing !== null && (roots.length > 1 || !isRoot(listing.path)));
  const empty = $derived(
    listing !== null && listing.dirs.length === 0 && listing.files.length === 0 && !loading,
  );

  function isRoot(path: string): boolean {
    return roots.some((root) => root.path === path);
  }
</script>

{#if request}
  <div class="picker" role="dialog" aria-modal="true" aria-label={request.title}>
    <header>
      <button class="up" aria-label="На уровень выше" disabled={!canGoUp} onclick={up}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M10 3L5 8l5 5" /></svg>
      </button>
      <div class="where">
        <h1>{request.title}</h1>
        <p>{place}</p>
      </div>
    </header>

    <div class="list">
      {#if failure}
        <p class="note">{failure}</p>
      {:else if !listing}
        {#each roots as root (root.path)}
          <button class="row" onclick={() => enter(root.path)}>
            <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 2.5h10a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1v-9a1 1 0 0 1 1-1M5 10.5h2" /></svg>
            <span>{root.name}</span>
          </button>
        {/each}
      {:else}
        {#each listing.dirs as dir (dir.path)}
          <button class="row" onclick={() => enter(dir.path)}>
            <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M1.5 4a1 1 0 0 1 1-1h3l1.5 1.5h6.5a1 1 0 0 1 1 1V12a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1z" /></svg>
            <span>{dir.name}</span>
          </button>
        {/each}
        {#each listing.files as file (file.path)}
          <button class="row file" class:chosen={chosen.includes(file.path)} onclick={() => toggle(file.path)}>
            <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M6 12.5V3.5l7-1.5v9M6 12.5a2 2 0 1 1-4 0 2 2 0 0 1 4 0M13 11a2 2 0 1 1-4 0 2 2 0 0 1 4 0" /></svg>
            <span>{file.name}</span>
          </button>
        {/each}
        {#if empty}
          <p class="note">
            {request.mode === 'folder' ? 'Вложенных папок нет' : 'Подходящих файлов здесь нет'}
          </p>
        {/if}
      {/if}
    </div>

    <footer>
      <button class="ghost" onclick={() => finish([])}>Отмена</button>
      {#if request.mode === 'folder'}
        <button class="primary" disabled={!listing} onclick={confirm}>Выбрать эту папку</button>
      {:else if request.multiple}
        <button class="primary" disabled={chosen.length === 0} onclick={confirm}>
          Добавить{chosen.length > 0 ? `: ${chosen.length}` : ''}
        </button>
      {/if}
    </footer>
  </div>
{/if}

<style>
  /* Выше модальных окон: выбор папки зовут и из них (источники плейлиста). */
  .picker {
    position: fixed;
    inset: 0;
    z-index: 40;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    /* Окно на телефоне идёт под системные панели — отступаем от них сами. */
    padding: env(safe-area-inset-top) env(safe-area-inset-right) env(safe-area-inset-bottom)
      env(safe-area-inset-left);
  }
  header {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 8px 12px 8px 4px;
    border-bottom: 1px solid var(--border);
  }
  .up {
    flex: none;
    width: 48px;
    height: 48px;
    display: grid;
    place-items: center;
    color: var(--fg);
    background: none;
    border: none;
    border-radius: 50%;
  }
  .up:disabled {
    opacity: 0.3;
  }
  .where {
    min-width: 0;
  }
  h1 {
    margin: 0;
    font-size: var(--fs-lg);
    font-weight: 600;
  }
  .where p {
    margin: 2px 0 0;
    font-size: var(--fs-sm);
    color: var(--fg-faint);
    /* Длинный путь режем слева: важен его конец — папка, в которой стоят. */
    direction: rtl;
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .list {
    flex: 1;
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  .row {
    width: 100%;
    /* 52 px — палец попадает в строку, не задевая соседние. */
    min-height: 52px;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 8px 16px;
    font: inherit;
    text-align: left;
    color: var(--fg);
    background: none;
    border: none;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
  }
  .row:active {
    background: var(--row-hover);
  }
  .row span {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .row.file {
    color: var(--fg-dim);
  }
  .row.chosen {
    color: var(--fg);
    background: var(--accent-soft);
  }
  svg {
    flex: none;
    width: 22px;
    height: 22px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.3;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .row svg {
    color: var(--fg-faint);
  }
  .row.chosen svg {
    color: var(--accent);
  }
  .note {
    margin: 32px 24px;
    text-align: center;
    color: var(--fg-faint);
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 16px;
    border-top: 1px solid var(--border);
  }
  footer button {
    min-height: 44px;
    padding: 8px 18px;
  }
</style>
