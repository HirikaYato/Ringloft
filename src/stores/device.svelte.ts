import { api, type PermissionKind, type Permissions } from '../lib/api';

/**
 * Устройство, на котором запущен плеер: телефон это или нет и что ему
 * разрешила система. На десктопе разрешено всё, и спрашивать нечего.
 */
class DeviceStore {
  mobile = $state(false);
  /** `null` — ещё не спрашивали. */
  permissions = $state<Permissions | null>(null);

  async init(mobile: boolean): Promise<void> {
    this.mobile = mobile;
    await this.refresh();
    if (!mobile) return;
    // Разрешение выдают на экране настроек системы, и ответ оттуда нам не
    // приходит — перечитываем сами, когда окно снова на экране.
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'visible') void this.refresh();
    });
  }

  async refresh(): Promise<void> {
    try {
      this.permissions = await api.permissionsStatus();
    } catch (err) {
      console.warn('[ringloft] не удалось узнать разрешения', err);
    }
  }

  async request(kind: PermissionKind): Promise<void> {
    try {
      await api.permissionsRequest(kind);
    } catch (err) {
      console.warn('[ringloft] не удалось попросить разрешение', err);
    }
  }
}

export const device = new DeviceStore();
