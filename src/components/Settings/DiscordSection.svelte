<script lang="ts">
  /** Discord Rich Presence: включить, выключить и проверить подключение. */
  import { api, toErrorPayload } from '../../lib/api';
  import { settings } from '../../stores/settings.svelte.ts';

  let checking = $state(false);
  let result = $state<string | null>(null);

  const enabled = $derived(settings.current?.discord.enabled ?? false);

  async function probe() {
    checking = true;
    result = null;
    try {
      await api.discordProbe();
      result = 'Discord отвечает — статус появится в профиле.';
    } catch (err) {
      result = `Не вышло: ${toErrorPayload(err).message}`;
    } finally {
      checking = false;
    }
  }
</script>

<section class="settings-block">
  <h2>Discord</h2>

  <label class="check">
    <input
      type="checkbox"
      checked={enabled}
      onchange={(event) => settings.patch({ discord: { enabled: event.currentTarget.checked } })}
    />
    <span>показывать в Discord, что играет</span>
  </label>

  {#if enabled}
    <div class="field">
      <span class="label"></span>
      <div class="control">
        <button class="chip" disabled={checking} onclick={() => void probe()}>
          {checking ? 'проверяю…' : 'проверить подключение'}
        </button>
        {#if result}<p class="hint">{result}</p>{/if}
      </div>
    </div>
  {/if}
</section>
