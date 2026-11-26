// The Analytics summary (`GET /api/analytics/summary`, operation `analytics.summary`): figures
// across runs, over each run's first branch, as the caller sees them.
import { get } from './client';

export type Group = 'setup' | 'model';

export interface Totals {
  runs: number;
  /** Runs that count for the pass rate: passed, failed or over budget. */
  judged: number;
  passed: number;
  pass_rate: number | null;
  cost_usd: number;
  cost_per_run: number | null;
  cost_per_pass: number | null;
  median_seconds: number | null;
  tokens: number;
  /** Runs whose cost is not known (left out of the cost figures). */
  uncosted: number;
}

export interface SeriesPoint { t: string; group: string; judged: number; passed: number; pass_rate: number | null }

export interface GroupPoint {
  group: string; runs: number; judged: number; passed: number; pass_rate: number | null;
  cost_usd: number; cost_per_run: number | null; tokens: number;
}

export interface BoardRow {
  setup: string; world: string; scenario: string; runs: number; judged: number; passed: number;
  pass_rate: number | null; cost_per_run: number | null; median_seconds: number | null; cost_per_pass: number | null;
}

export interface Options { world: string[]; scenario: string[]; setup: string[]; model: string[]; status: string[] }

export interface Summary {
  group: Group;
  bucket: 'day' | 'week';
  totals: Totals;
  /** Ended runs whose figures are still being worked out, and runs still going. */
  pending: { unprocessed: number; running: number };
  series: SeriesPoint[];
  points: GroupPoint[];
  leaderboard: BoardRow[];
  options: Options;
}

/** The summary for a query string the filters made (`?from=…&setup=a,b&group=model`). */
export const analyticsSummary = (query: string) => get<Summary>(`/api/analytics/summary${query}`);
