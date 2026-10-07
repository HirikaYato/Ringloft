<script lang="ts" module>
  /** Контурные значки на сетке 16×16 — в том же духе, что у десктопа. */
  const STROKE = {
    playlist: 'M2.5 4h8M2.5 7.5h8M2.5 11h5M13 3v7.5',
    library: 'M8 14A6 6 0 1 0 8 2a6 6 0 0 0 0 12zM8 9.5a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3z',
    settings: 'M2.5 4.5h6M11.5 4.5h2M2.5 11.5h2M7.5 11.5h6M10 3v3M6 10v3',
    search: 'M7 12A5 5 0 1 0 7 2a5 5 0 0 0 0 10zM10.6 10.6L14 14',
    plus: 'M8 3v10M3 8h10',
    down: 'M3.5 6l4.5 4.5L12.5 6',
    back: 'M10 3L5 8l5 5',
    forward: 'M6 3l5 5-5 5',
    lyrics:
      'M3.5 2.5h9a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1v-9a1 1 0 0 1 1-1M5 5.5h6M5 8h6M5 10.5h3.5',
    queue: 'M2.5 4h8M2.5 7.5h8M2.5 11h5M12 9.2l3.2 2-3.2 2z',
    moon: 'M13 9.5A5.5 5.5 0 0 1 6.5 3a5.5 5.5 0 1 0 6.5 6.5z',
    eq: 'M3.5 2v4M3.5 10v4M8 2v7M8 13v1M12.5 2v2M12.5 8v6',
    folder: 'M1.5 4a1 1 0 0 1 1-1h3l1.5 1.5h6.5a1 1 0 0 1 1 1V12a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1z',
    note: 'M6 12.5V3.5l7-1.5v9M6 12.5a2 2 0 1 1-4 0 2 2 0 0 1 4 0M13 11a2 2 0 1 1-4 0 2 2 0 0 1 4 0',
    radio:
      'M8 9.5a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3zM4.8 11.2a4.5 4.5 0 0 1 0-6.4M11.2 4.8a4.5 4.5 0 0 1 0 6.4M2.7 13.3a7.5 7.5 0 0 1 0-10.6M13.3 2.7a7.5 7.5 0 0 1 0 10.6',
    close: 'M4 4l8 8M12 4l-8 8',
    check: 'M3 8.5l3.2 3.2L13 5',
    trash: 'M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.6 8a1 1 0 0 0 1 .9h3.8a1 1 0 0 0 1-.9l.6-8',
    shuffle: 'M2.5 4.5h2.2l6.1 7h2.7M2.5 11.5h2.2l6.1-7h2.7M11.8 2.6l1.9 1.9-1.9 1.9M11.8 9.6l1.9 1.9-1.9 1.9',
    repeat: 'M4.5 4.5h7a2 2 0 0 1 2 2v1M11.5 11.5h-7a2 2 0 0 1-2-2v-1M10 2.6l1.9 1.9L10 6.4M6 9.6L4.1 11.5 6 13.4',
    album: 'M2.5 2.5h11v11h-11zM8 10.2a2.2 2.2 0 1 0 0-4.4 2.2 2.2 0 0 0 0 4.4z',
    artist: 'M8 8a2.8 2.8 0 1 0 0-5.6A2.8 2.8 0 0 0 8 8zM2.8 13.6a5.2 5.2 0 0 1 10.4 0',
    genre: 'M2.5 2.5h5l6 6-5 5-6-6zM5.3 5.3h.01',
    smart: 'M9 1.8L3.5 9h4L7 14.2 12.5 7h-4z',
    stats: 'M3 13V8M8 13V3M13 13V6',
    refresh: 'M13 8a5 5 0 1 1-1.5-3.5M13 2.5v2.5h-2.5',
    edit: 'M2.5 13.5l.6-2.8 7.6-7.6 2.2 2.2-7.6 7.6zM9.4 4.4l2.2 2.2',
  } as const;

  /** Залитые — транспорт: им нужен вес, контур на кнопке «играть» теряется. */
  const FILL = {
    play: 'M4 2.8v10.4c0 .7.8 1.2 1.4.8l8-5.2c.6-.4.6-1.2 0-1.6l-8-5.2c-.6-.4-1.4 0-1.4.8z',
    pause:
      'M4.5 2.5H6a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H4.5a1 1 0 0 1-1-1v-9a1 1 0 0 1 1-1zM10 2.5h1.5a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H10a1 1 0 0 1-1-1v-9a1 1 0 0 1 1-1z',
    next: 'M4 3.2v9.6c0 .6.7 1 1.2.6l6.4-4.8a.8.8 0 0 0 0-1.2L5.2 2.6c-.5-.4-1.2 0-1.2.6zM11.4 3.8a.8.8 0 0 1 1.6 0v8.4a.8.8 0 0 1-1.6 0z',
    prev: 'M12 3.2v9.6c0 .6-.7 1-1.2.6L4.4 8.6a.8.8 0 0 1 0-1.2l6.4-4.8c.5-.4 1.2 0 1.2.6zM3 3.8a.8.8 0 0 1 1.6 0v8.4a.8.8 0 0 1-1.6 0z',
    more: 'M8 4.6a1.2 1.2 0 1 0 0-2.4 1.2 1.2 0 0 0 0 2.4zM8 9.2a1.2 1.2 0 1 0 0-2.4 1.2 1.2 0 0 0 0 2.4zM8 13.8a1.2 1.2 0 1 0 0-2.4 1.2 1.2 0 0 0 0 2.4z',
  } as const;

  export type IconName = keyof typeof STROKE | keyof typeof FILL;

  function isFilled(name: IconName): name is keyof typeof FILL {
    return name in FILL;
  }
</script>

<script lang="ts">
  const { name, size = 22 }: { name: IconName; size?: number } = $props();

  const filled = $derived(isFilled(name));
  const path = $derived(isFilled(name) ? FILL[name] : STROKE[name]);
</script>

<svg viewBox="0 0 16 16" width={size} height={size} class:filled aria-hidden="true">
  <path d={path} />
</svg>

<style>
  svg {
    flex: none;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.3;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  svg.filled {
    fill: currentColor;
    stroke: none;
  }
</style>
