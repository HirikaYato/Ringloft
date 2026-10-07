/**
 * Просьба выбрать папку или файлы в своём окне выбора (`Mobile/PathPicker`).
 * Оно одно на приложение, поэтому и просьба одна: стор держит её и обещание,
 * которого ждёт просивший. Звать напрямую не нужно — есть `lib/pick.ts`.
 */
export interface PickRequest {
  /** `folder` — выбирают папку, в которой стоят; `files` — отмечают файлы. */
  mode: 'folder' | 'files';
  title: string;
  /** Какие файлы показывать. В режиме папки — пусто. */
  extensions: string[];
  multiple: boolean;
}

class PickerStore {
  request = $state<PickRequest | null>(null);
  private resolve: ((paths: string[]) => void) | null = null;

  /** Пустой ответ — отмена. */
  open(request: PickRequest): Promise<string[]> {
    // Прежнюю просьбу никто уже не увидит — отвечаем на неё отменой.
    this.finish([]);
    this.request = request;
    return new Promise((resolve) => (this.resolve = resolve));
  }

  finish(paths: string[]): void {
    const resolve = this.resolve;
    this.resolve = null;
    this.request = null;
    resolve?.(paths);
  }
}

export const picker = new PickerStore();
