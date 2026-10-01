<script lang="ts">
  /** Ход подсчёта громкости словами. Вид — общий `TaskBar`. */
  import TaskBar from './TaskBar.svelte';
  import { plural } from '../../lib/format';
  import { gain } from '../../stores/gain.svelte.ts';

  const progress = $derived(gain.progress);
  const report = $derived(gain.last);

  const label = $derived(
    progress?.phase === 'writing' ? 'Записываю метки' : 'Считаю громкость',
  );

  const note = $derived.by(() => {
    if (!report) return '';
    const parts = [
      `посчитано ${report.measured} ${plural(report.measured, 'файл', 'файла', 'файлов')}`,
    ];
    if (report.skipped > 0) parts.push(`метка уже была у ${report.skipped}`);
    if (report.failed > 0) parts.push(`не удалось ${report.failed}`);
    return (report.cancelled ? 'Подсчёт остановлен: ' : 'Громкость посчитана: ') + parts.join(', ');
  });
</script>

<TaskBar
  {label}
  {note}
  running={gain.running}
  processed={progress?.processed ?? 0}
  total={progress?.total ?? 0}
  current={progress?.current ?? null}
  failed={(report?.failed ?? 0) > 0}
  onstop={() => gain.cancel()}
  onclose={() => (gain.last = null)}
/>
