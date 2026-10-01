<script lang="ts">
  /**
   * Ключ AcoustID для тегов по звуку. Заводит пользователь — как и ключ
   * Last.fm: бесплатно, на acoustid.org/new-application.
   */
  import { settings } from '../../stores/settings.svelte.ts';

  const key = $derived(settings.current?.acoustid.apiKey ?? '');
</script>

<section class="settings-block">
  <h2>AcoustID</h2>
  <p class="hint">
    Теги по звуку: плеер снимает отпечаток трека и находит его в базе AcoustID и MusicBrainz —
    название, исполнителя, альбом, год и номер. Нужен свой ключ приложения: войти на
    acoustid.org → «Your applications» → «New application», имя любое (например, Ringloft).
  </p>
  <div class="field">
    <span class="label">Ключ приложения</span>
    <div class="control">
      <input
        class="input"
        type="text"
        spellcheck="false"
        placeholder="ключ из acoustid.org"
        value={key}
        onchange={(event) =>
          settings.patch({ acoustid: { apiKey: event.currentTarget.value.trim() || null } })}
      />
      <p class="hint">
        Потом — правый клик по трекам в плейлисте → «Определить теги по звуку». Сам плеер ничего
        не переписывает: варианты показываются, выбираешь ты.
      </p>
    </div>
  </div>
</section>
