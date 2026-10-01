<script lang="ts">
  /** Ход пакетной загрузки текстов. Вид — общий `TaskBar`. */
  import TaskBar from './TaskBar.svelte';
  import { plural } from '../../lib/format';
  import { lyricsScan } from '../../stores/lyrics.svelte.ts';

  const progress = $derived(lyricsScan.progress);
  const report = $derived(lyricsScan.last);

  const note = $derived.by(() => {
    if (!report) return '';
    const parts = [
      `нашёл ${report.found} ${plural(report.found, 'текст', 'текста', 'текстов')}`,
    ];
    if (report.skipped > 0) parts.push(`уже было у ${report.skipped}`);
    if (report.missing > 0) parts.push(`нет в базе у ${report.missing}`);
    if (report.failed > 0) parts.push(`не удалось ${report.failed}`);
    return (report.cancelled ? 'Загрузка остановлена: ' : 'Тексты загружены: ') + parts.join(', ');
  });
</script>

<TaskBar
  label="Ищу текст"
  {note}
  running={lyricsScan.running}
  processed={progress?.processed ?? 0}
  total={progress?.total ?? 0}
  current={progress?.current ?? null}
  failed={(report?.failed ?? 0) > 0}
  onstop={() => lyricsScan.cancel()}
  onclose={() => (lyricsScan.last = null)}
/>
