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

export interface SeriesPoint {
  t: string; group: string; judged: number; passed: number; pass_rate: number | null;
  runs: number; cost_usd: number | null; uncosted: number; tokens: number;
}
export interface FailureCount { name: string; runs: number }
export interface DurationBin { lower_seconds: number; upper_seconds: number | null; runs: number }

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
  failures: { checks: FailureCount[]; gates: FailureCount[]; reasons: FailureCount[] };
  duration: DurationBin[];
}

/** The summary for a query string the filters made (`?from=…&setup=a,b&group=model`). */
export const analyticsSummary = (query: string) => get<Summary>(`/api/analytics/summary${query}`);

// Search over every run's steps (`GET /api/analytics/search`, operation `analytics.search`).

/** One step found; `snippet` marks each match between U+E000 and U+E001. */
export interface SearchHit {
  run_id: string; run_name: string; branch_id: string; seq: number;
  kind: string; agent: string | null; at: string; snippet: string;
}
export interface FacetCount { name: string; count: number }

export interface SearchResults {
  hits: SearchHit[];
  /** Matches in all, counted up to 10,000 (`total_capped` when there are more). */
  total: number;
  total_capped: boolean;
  /** The next page's offset; null on the last. */
  next_offset: number | null;
  /** Matches per kind and per agent, whatever kinds and agents are picked. */
  kinds: FacetCount[];
  agents: FacetCount[];
}

/** One page of steps for a query string the search made (`?q=crew&kind=["reasoning"]&offset=50`). */
export const searchSteps = (query: string) => get<SearchResults>(`/api/analytics/search${query}`);
