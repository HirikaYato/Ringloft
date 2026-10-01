<script lang="ts">
  import { Channel } from '@tauri-apps/api/core';
  import { onDestroy, onMount } from 'svelte';

  import { api, toErrorPayload } from '../../lib/api';

  /** `bare` — без подложки и рамки: так он выглядит поверх обложки. */
  let { bare = false }: { bare?: boolean } = $props();

  let canvas: HTMLCanvasElement | null = $state(null);
  let levels: number[] = [];
  let frame = 0;
  let colors = { bar: '#4f8cff', dim: '#2a2d35' };

  onMount(async () => {
    try {
      const channel = new Channel<number[]>();
      channel.onmessage = (data) => {
        levels = data;
      };
      await api.spectrumSubscribe(channel);
    } catch (err) {
      console.error('[ringloft] спектр не подписался', toErrorPayload(err));
    }
    readColors();
    frame = requestAnimationFrame(draw);
  });

  onDestroy(() => {
    cancelAnimationFrame(frame);
    // Пока никто не смотрит, бэкенд не должен считать БПФ.
    void api.spectrumUnsubscribe();
  });

  /** Цвета берём из темы, чтобы холст не выбивался из оформления. */
  function readColors() {
    if (!canvas) return;
    const style = getComputedStyle(canvas);
    colors = {
      bar: style.getPropertyValue('--accent').trim() || colors.bar,
      dim: style.getPropertyValue('--bg-inset').trim() || colors.dim,
    };
  }

  /** Акцент может смениться на ходу (цвет из обложки) — перечитываем его
      раз в полсекунды: одна выборка стиля, а не на каждый кадр. */
  let drawn = 0;

  function draw() {
    frame = requestAnimationFrame(draw);
    drawn += 1;
    if (drawn % 30 === 0) readColors();
    const context = canvas?.getContext('2d');
    if (!canvas || !context) return;

    const ratio = window.devicePixelRatio || 1;
    const width = canvas.clientWidth;
    const height = canvas.clientHeight;
    if (canvas.width !== width * ratio || canvas.height !== height * ratio) {
      canvas.width = width * ratio;
      canvas.height = height * ratio;
    }

    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    context.clearRect(0, 0, width, height);
    if (levels.length === 0) return;

    const gap = 2;
    const barWidth = Math.max(1, (width - gap * (levels.length - 1)) / levels.length);
    const gradient = context.createLinearGradient(0, height, 0, 0);
    gradient.addColorStop(0, colors.bar);
    gradient.addColorStop(1, colors.dim);

    for (let index = 0; index < levels.length; index += 1) {
      const value = Math.max(0, Math.min(1, levels[index] ?? 0));
      const barHeight = Math.max(1, value * (height - 2));
      context.fillStyle = gradient;
      context.fillRect(index * (barWidth + gap), height - barHeight, barWidth, barHeight);
    }
  }
</script>

<div class="spectrum" class:bare>
  <canvas bind:this={canvas}></canvas>
</div>

<style>
  .spectrum {
    height: 96px;
    padding: 8px 12px;
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    flex: none;
  }
  .spectrum.bare {
    background: none;
    border: none;
    padding: 0;
    height: 72px;
  }
  canvas {
    display: block;
    width: 100%;
    height: 100%;
  }
</style>
