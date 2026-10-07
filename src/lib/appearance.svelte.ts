/**
 * Тема, акцент и шрифт — из настроек в CSS. Общее для обоих корневых
 * макетов; зовётся из корневого компонента при его создании, потому что
 * заводит эффекты.
 */
import { player } from '../stores/player.svelte.ts';
import { applyAccent, applyTheme, resolvedTheme, settings } from '../stores/settings.svelte.ts';
import { accentFromTint } from './cover-color';
import { applyTypography } from './typography.svelte.ts';

export function useAppearance(): void {
  $effect(() => {
    const theme = settings.current?.ui.theme;
    if (theme) applyTheme(theme);
  });

  // Акцент из обложки — поверх своего: свой остаётся запасным для треков
  // без обложки и с бесцветной.
  $effect(() => {
    const ui = settings.current?.ui;
    const tint = ui?.accentFromCover ? player.coverTint : null;
    const fromCover = tint ? accentFromTint(tint, resolvedTheme(ui?.theme ?? 'dark')) : null;
    applyAccent(fromCover ?? ui?.accent);
  });

  $effect(() => {
    const ui = settings.current?.ui;
    if (ui) applyTypography(ui.fontSize, ui.font, ui.lyricsFont);
  });
}
