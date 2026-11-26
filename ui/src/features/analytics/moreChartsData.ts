import type { DurationBin, SeriesPoint } from '@/api/analytics';

export interface UsageBucket { t: string; runs: number; tokens: number; cost_usd: number | null; uncosted: number }
/** Sum disjoint setup/model groups in each UTC bucket; missing cost never becomes a zero. */
export function usageBuckets(series: SeriesPoint[]): UsageBucket[] {
  const byTime = new Map<string, UsageBucket>();
  for (const p of series) {
    const b = byTime.get(p.t) ?? { t: p.t, runs: 0, tokens: 0, cost_usd: null, uncosted: 0 };
    b.runs += p.runs;
    b.tokens += p.tokens;
    b.uncosted += p.uncosted;
    if (p.cost_usd !== null) b.cost_usd = (b.cost_usd ?? 0) + p.cost_usd;
    byTime.set(p.t, b);
  }
  return [...byTime.values()].sort((a, b) => Date.parse(a.t) - Date.parse(b.t));
}

const time = (seconds: number) => seconds < 60 ? `${seconds} s` : seconds < 3600 ? `${seconds / 60} min` : `${seconds / 3600} h`;
export function durationLabel(bin: DurationBin): string {
  if (bin.upper_seconds === null) return `${time(bin.lower_seconds)} or more`;
  return `${time(bin.lower_seconds)} to < ${time(bin.upper_seconds)}`;
}
export const bucketDate = (t: string) => new Date(t).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric', timeZone: 'UTC' });
export const costValue = (v: number | null) => v === null ? 'Unknown' : `$${v.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 4 })}`;
