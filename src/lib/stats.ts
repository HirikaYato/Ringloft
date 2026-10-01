/**
 * Подготовка итогов к показу: бэкенд отдаёт только дни (месяцы), в которые
 * что-то слушали, а графику нужны и пустые — иначе столбики съезжаются.
 */
import type { StatsBucket, StatsPeriod } from './api';

export type ShownBucket = { label: string; plays: number; title: string; short: string };

const MONTHS = ['янв', 'фев', 'мар', 'апр', 'май', 'июн', 'июл', 'авг', 'сен', 'окт', 'ноя', 'дек'];

const pad = (value: number) => String(value).padStart(2, '0');
const dayKey = (date: Date) => `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
const monthKey = (date: Date) => `${date.getFullYear()}-${pad(date.getMonth() + 1)}`;

export function fillBuckets(period: StatsPeriod, buckets: StatsBucket[]): ShownBucket[] {
  const counts = new Map(buckets.map((bucket) => [bucket.label, bucket.plays]));
  const now = new Date();
  const out: ShownBucket[] = [];

  if (period === 'week' || period === 'month') {
    const days = period === 'week' ? 7 : 30;
    for (let back = days - 1; back >= 0; back -= 1) {
      const date = new Date(now.getFullYear(), now.getMonth(), now.getDate() - back);
      const label = dayKey(date);
      const short = `${date.getDate()} ${MONTHS[date.getMonth()]}`;
      out.push({ label, plays: counts.get(label) ?? 0, title: short, short });
    }
    return out;
  }

  // Год — двенадцать месяцев; всё время — от первого месяца с прослушиваниями.
  const first = buckets[0]?.label;
  const start =
    period === 'year' || !first
      ? new Date(now.getFullYear(), now.getMonth() - 11, 1)
      : new Date(Number(first.slice(0, 4)), Number(first.slice(5, 7)) - 1, 1);
  for (let date = start; date <= now; date = new Date(date.getFullYear(), date.getMonth() + 1, 1)) {
    const label = monthKey(date);
    const short = `${MONTHS[date.getMonth()]} ${date.getFullYear()}`;
    out.push({ label, plays: counts.get(label) ?? 0, title: short, short });
  }
  return out;
}

/** «14 ч 05 мин», «38 мин». */
export function hoursText(ms: number): string {
  const minutes = Math.round(ms / 60_000);
  const hours = Math.floor(minutes / 60);
  return hours > 0 ? `${hours} ч ${pad(minutes % 60)} мин` : `${minutes} мин`;
}
