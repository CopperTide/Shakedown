// The Analytics filter: a time range and the world, scenario, setup and model picks, kept in the
// page's address (repeated keys, so names with commas survive); the API query it makes; and the
// drill-down addresses into the runs list. Pure functions only.
import type { Group } from '@/api/analytics';

export type Range = '7d' | '30d' | '90d' | 'all';
export const RANGES: readonly { id: Range; label: string }[] = [
  { id: '7d', label: '7 d' }, { id: '30d', label: '30 d' }, { id: '90d', label: '90 d' }, { id: 'all', label: 'All' },
];
export const DEFAULT_RANGE: Range = '30d';

export const DIMENSIONS = ['world', 'scenario', 'setup', 'model'] as const;
export type Dimension = (typeof DIMENSIONS)[number];
export const DIMENSION_LABEL: Record<Dimension, string> = { world: 'World', scenario: 'Scenario', setup: 'Setup', model: 'Model' };

export interface AnalyticsFilter {
  range: Range;
  world: string[];
  scenario: string[];
  setup: string[];
  model: string[];
  /** What the pass-rate lines and the cost dots are per. */
  group: Group;
}

const DAYS: Record<Exclude<Range, 'all'>, number> = { '7d': 7, '30d': 30, '90d': 90 };

/** The filter in `sp`; anything unknown falls back to the default. */
export function parseFilter(sp: URLSearchParams): AnalyticsFilter {
  const range = RANGES.some((r) => r.id === sp.get('range')) ? (sp.get('range') as Range) : DEFAULT_RANGE;
  const pick = (k: Dimension) => [...new Set(sp.getAll(k).filter(Boolean))];
  return { range, world: pick('world'), scenario: pick('scenario'), setup: pick('setup'), model: pick('model'), group: sp.get('group') === 'model' ? 'model' : 'setup' };
}

/** `prev` with `f` written in (defaults left out, other keys kept). */
export function writeFilter(prev: URLSearchParams, f: AnalyticsFilter): URLSearchParams {
  const sp = new URLSearchParams(prev);
  for (const k of ['range', 'group', ...DIMENSIONS]) sp.delete(k);
  if (f.range !== DEFAULT_RANGE) sp.set('range', f.range);
  for (const k of DIMENSIONS) for (const v of f[k]) sp.append(k, v);
  if (f.group !== 'setup') sp.set('group', f.group);
  return sp;
}

/** True when some world, scenario, setup or model is picked. */
export const isNarrowed = (f: AnalyticsFilter) => DIMENSIONS.some((k) => f[k].length > 0);

/** The start of the range: midnight UTC, `days - 1` days before `now` (so it holds for the whole day); null for all. */
export function fromOf(range: Range, now: Date): string | null {
  if (range === 'all') return null;
  const d = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate()));
  d.setUTCDate(d.getUTCDate() - (DAYS[range] - 1));
  return d.toISOString();
}

/** The API's query string for `f` at `now`: each list as a JSON array, since names can hold commas. */
export function apiQuery(f: AnalyticsFilter, now: Date): string {
  const params = new URLSearchParams();
  const from = fromOf(f.range, now);
  if (from) params.set('from', from);
  for (const k of DIMENSIONS) if (f[k].length) params.set(k, JSON.stringify(f[k]));
  params.set('group', f.group);
  return `?${params.toString()}`;
}

/** What a drill-down narrows the runs list to. */
export interface RunsDrill { setup?: string; scenario?: string; model?: string }

/** The runs list showing the runs a chart mark or a leaderboard row is made of (the page's other picks carried along). */
export function runsHref(f: AnalyticsFilter, now: Date, drill: RunsDrill): string {
  const params = new URLSearchParams();
  const from = fromOf(f.range, now);
  if (from) params.set('from', from);
  const one = (k: 'setup' | 'scenario' | 'model') => drill[k] ?? (f[k].length === 1 ? f[k][0] : undefined);
  for (const k of ['setup', 'scenario', 'model'] as const) {
    const v = one(k);
    if (v) params.set(k, v);
  }
  return `/runs?${params.toString()}`;
}
