/**
 * Доминирующий цвет обложки — им подкрашивается нижняя панель.
 *
 * Считаем по уменьшенной копии (32×32): это одна отрисовка в canvas вместо
 * миллиона пикселей, а для пятна под панелью точности хватает с запасом.
 * Картинки тут только наши собственные blob-адреса, так что canvas не портится.
 */
const SIZE = 32;

export async function dominantColor(url: string): Promise<string | null> {
  try {
    const image = await load(url);
    const canvas = document.createElement('canvas');
    canvas.width = SIZE;
    canvas.height = SIZE;
    const context = canvas.getContext('2d', { willReadFrequently: true });
    if (!context) return null;
    context.drawImage(image, 0, 0, SIZE, SIZE);
    return pick(context.getImageData(0, 0, SIZE, SIZE).data);
  } catch {
    return null;
  }
}

function load(url: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error('обложка не загрузилась'));
    image.src = url;
  });
}

/**
 * Берём самый «живой» цвет, а не средний: усреднение по всей картинке даёт
 * унылую серо-бурую кашу. Вес — насыщенность, приглушённая к краям яркости,
 * чтобы не выбрать чёрную рамку или белый угол.
 */
function pick(pixels: Uint8ClampedArray): string | null {
  let bestScore = 0;
  let best: { r: number; g: number; b: number } | null = null;

  for (let offset = 0; offset < pixels.length; offset += 4) {
    const alpha = pixels[offset + 3] ?? 0;
    if (alpha < 200) continue;
    const r = pixels[offset] ?? 0;
    const g = pixels[offset + 1] ?? 0;
    const b = pixels[offset + 2] ?? 0;
    const max = Math.max(r, g, b);
    const min = Math.min(r, g, b);
    if (max === 0) continue;
    const saturation = (max - min) / max;
    const lightness = max / 255;
    const score = saturation * (1 - Math.abs(lightness - 0.55) * 1.4);
    if (score > bestScore) {
      bestScore = score;
      best = { r, g, b };
    }
  }

  // Обложка почти без цвета (чёрно-белое фото, скан конверта) — подкрашивать
  // нечем, панель останется обычной.
  if (!best || bestScore < 0.1) return null;
  return `rgb(${best.r} ${best.g} ${best.b})`;
}

/**
 * Цвет обложки как акцент. Сам по себе он бывает каким угодно — тёмно-бурым,
 * почти белым, — а акцентом рисуются ползунки, выделение и подписи, и они
 * должны читаться на фоне темы. Поэтому оттенок берём от обложки, а
 * насыщенность и яркость подтягиваем в рабочий диапазон.
 */
export function accentFromTint(tint: string, theme: 'dark' | 'light'): string | null {
  const parts = tint.match(/\d+/g)?.map(Number);
  if (!parts || parts.length < 3) return null;
  const [h, s, l] = toHsl(parts[0] ?? 0, parts[1] ?? 0, parts[2] ?? 0);
  const saturation = Math.min(0.85, Math.max(0.5, s));
  const lightness = theme === 'dark' ? Math.min(0.66, Math.max(0.56, l)) : Math.min(0.46, Math.max(0.36, l));
  return toHex(h, saturation, lightness);
}

function toHsl(r: number, g: number, b: number): [number, number, number] {
  const [red, green, blue] = [r / 255, g / 255, b / 255];
  const max = Math.max(red, green, blue);
  const min = Math.min(red, green, blue);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h: number;
  if (max === red) h = (green - blue) / d + (green < blue ? 6 : 0);
  else if (max === green) h = (blue - red) / d + 2;
  else h = (red - green) / d + 4;
  return [h * 60, s, l];
}

function toHex(h: number, s: number, l: number): string {
  const a = s * Math.min(l, 1 - l);
  const channel = (n: number) => {
    const k = (n + h / 30) % 12;
    const value = l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1));
    return Math.round(value * 255).toString(16).padStart(2, '0');
  };
  return `#${channel(0)}${channel(8)}${channel(4)}`;
}
