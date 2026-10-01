<script lang="ts">
  import { api, toErrorPayload, type LastfmStatus } from '../../lib/api';
  import { plural } from '../../lib/format';
  import { settings } from '../../stores/settings.svelte.ts';

  let status = $state<LastfmStatus | null>(null);
  let waiting = $state(false);
  /** Ссылка открыта, ждём, пока её подтвердят в браузере. */
  let pendingAuth = $state(false);
  let error = $state<string | null>(null);

  const config = $derived(settings.current?.lastfm);

  $effect(() => {
    void refresh();
  });

  async function refresh() {
    try {
      status = await api.lastfmStatus();
    } catch (err) {
      error = toErrorPayload(err).message;
    }
  }

  async function run(action: () => Promise<LastfmStatus | string>) {
    waiting = true;
    error = null;
    try {
      const result = await action();
      if (typeof result === 'string') pendingAuth = true;
      else {
        status = result;
        pendingAuth = false;
      }
      await settings.load();
    } catch (err) {
      error = toErrorPayload(err).message;
    } finally {
      waiting = false;
    }
  }
</script>

<section class="settings-block">
  <h2>Last.fm</h2>

  {#if status?.connected}
    <div class="field">
      <span class="label">Учётная запись</span>
      <div class="control">
        <p class="value">{status.username ?? 'подключена'}</p>
        {#if status.pending > 0}
          <p class="hint">
            {status.pending}
            {plural(status.pending, 'прослушивание ждёт', 'прослушивания ждут', 'прослушиваний ждут')}
            отправки.
          </p>
        {/if}
        <button class="chip" disabled={waiting} onclick={() => run(() => api.lastfmDisconnect())}>
          отключить
        </button>
      </div>
    </div>

    <label class="check">
      <input
        type="checkbox"
        checked={config?.enabled ?? false}
        onchange={(event) => settings.patch({ lastfm: { enabled: event.currentTarget.checked } })}
      />
      <span>отправлять прослушанное</span>
    </label>
  {:else}
    <p class="hint">
      Ключ и секрет нужно завести самому на last.fm/api/account/create — чужой ключ в плеере это
      чужая квота.
    </p>

    <div class="field">
      <span class="label">API key</span>
      <div class="control">
        <input
          class="input"
          type="text"
          value={config?.apiKey ?? ''}
          onchange={(event) => settings.patch({ lastfm: { apiKey: event.currentTarget.value } })}
        />
      </div>
    </div>

    <div class="field">
      <span class="label">API secret</span>
      <div class="control">
        <input
          class="input"
          type="password"
          value={config?.apiSecret ?? ''}
          onchange={(event) => settings.patch({ lastfm: { apiSecret: event.currentTarget.value } })}
        />
      </div>
    </div>

    <div class="field">
      <span class="label"></span>
      <div class="control">
        {#if pendingAuth}
          <p class="hint">
            Подтвердите доступ на открывшейся странице и нажмите «готово».
          </p>
          <div class="row">
            <button class="chip" disabled={waiting} onclick={() => run(() => api.lastfmFinishAuth())}>
              готово
            </button>
            <button class="chip" disabled={waiting} onclick={() => run(() => api.lastfmBeginAuth())}>
              открыть ещё раз
            </button>
          </div>
        {:else}
          <button class="chip" disabled={waiting} onclick={() => run(() => api.lastfmBeginAuth())}>
            {waiting ? 'жду Last.fm…' : 'подключить'}
          </button>
        {/if}
      </div>
    </div>
  {/if}

  {#if error}<p class="error">{error}</p>{/if}
</section>

<style>
  .value {
    margin: 4px 0 8px;
  }
  .error {
    margin: 8px 0 0;
    font-size: var(--fs-sm);
    color: var(--danger);
  }
</style>
