<script lang="ts">
  /** Ход пакетного поиска обложек. Вид — общий `TaskBar`. */
  import TaskBar from './TaskBar.svelte';
  import { covers } from '../../stores/covers.svelte.ts';

  const progress = $derived(covers.progress);
  const report = $derived(covers.last);

  const note = $derived.by(() => {
    if (!report) return '';
    const parts = [`нашёл ${report.found}`];
    if (report.missing > 0) parts.push(`нигде нет у ${report.missing}`);
    if (report.failed > 0) parts.push(`не удалось ${report.failed}`);
    return (report.cancelled ? 'Поиск обложек остановлен: ' : 'Обложки: ') + parts.join(', ');
  });
</script>

<TaskBar
  label="Ищу обложку"
  {note}
  running={covers.running}
  processed={progress?.processed ?? 0}
  total={progress?.total ?? 0}
  current={progress?.current ?? null}
  failed={(report?.failed ?? 0) > 0}
  onstop={() => covers.cancel()}
  onclose={() => (covers.last = null)}
/>
