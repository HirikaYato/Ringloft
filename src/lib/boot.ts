/**
 * Подъём сторов — общий для десктопа и телефона: корневые макеты у них
 * разные, а состояние одно.
 */
import { covers } from '../stores/covers.svelte.ts';
import { gain } from '../stores/gain.svelte.ts';
import { library } from '../stores/library.svelte.ts';
import { lyricsScan } from '../stores/lyrics.svelte.ts';
import { player } from '../stores/player.svelte.ts';
import { playlist } from '../stores/playlist.svelte.ts';
import { settings } from '../stores/settings.svelte.ts';
import { sleep } from '../stores/sleep.svelte.ts';

export async function initStores(): Promise<void> {
  await settings.load();
  await player.init();
  await playlist.init();
  await library.init(settings.current?.library.folders ?? []);
  await gain.init();
  await lyricsScan.init();
  await sleep.init();
  await covers.init();
}
