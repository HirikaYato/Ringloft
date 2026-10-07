/**
 * Выбор папок и файлов — одно место на обе платформы. На десктопе это
 * системный диалог. На телефоне он не годится: Android отдаёт из него не
 * пути, а адреса `content://`, а плеер ходит по диску сам — там открывается
 * своё окно выбора.
 *
 * Отмена — пустой список, а не `null`: вызывающему почти всегда всё равно.
 */
import { open } from '@tauri-apps/plugin-dialog';

import { device } from '../stores/device.svelte.ts';
import { picker } from '../stores/picker.svelte.ts';

export interface FileFilter {
  name: string;
  extensions: string[];
}

function asList(selected: string | string[] | null): string[] {
  if (Array.isArray(selected)) return selected;
  return selected ? [selected] : [];
}

export async function pickFolders(options: { title: string; multiple?: boolean }): Promise<string[]> {
  const multiple = options.multiple ?? false;
  if (device.mobile) {
    // На телефоне папку выбирают, стоя в ней, — поэтому всегда одну.
    return picker.open({ mode: 'folder', title: options.title, extensions: [], multiple: false });
  }
  return asList(await open({ multiple, directory: true }));
}

export async function pickFiles(options: {
  title: string;
  filter: FileFilter;
  multiple?: boolean;
}): Promise<string[]> {
  const multiple = options.multiple ?? false;
  if (device.mobile) {
    return picker.open({
      mode: 'files',
      title: options.title,
      extensions: options.filter.extensions,
      multiple,
    });
  }
  return asList(await open({ multiple, directory: false, filters: [options.filter] }));
}
