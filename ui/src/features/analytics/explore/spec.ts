// The Explore builder's state: measures, up to two dimensions, filters and the chart, kept in the
// page's address (repeated keys, so names with commas survive); the API spec it makes; presets.
// Pure functions only.
import type { BuilderSpec, Chart, DimensionId, MeasureId, Range, Spec } from '@/api/explore';
import { DEFAULT_RANGE, fromOf, RANGES } from '../filters';

export const MEASURE_IDS: readonly MeasureId[] = ['runs', 'pass_rate', 'cost', 'tokens_in', 'tokens_out', 'tokens_cached',
  'model_calls', 'tool_calls', 'refusals', 'errors', 'duration', 'thinking', 'steps'];
export const DIMENSION_IDS: readonly DimensionId[] = ['agent', 'model', 'setup', 'scenario', 'world', 'experiment', 'tool', 'kind', 'day'];
export const CHARTS: readonly { id: Chart; label: string }[] = [
  { id: 'bar', label: 'Bar' }, { id: 'stacked', label: 'Stacked' }, { id: 'line', label: 'Line' }, { id: 'table', label: 'Table' },
];
export const MAX_DIMENSIONS = 2;
export const MAX_MEASURES = 8;

export interface BuilderState {
  measures: MeasureId[];
  dimensions: DimensionId[];
  range: Range;
  experiment: string[];
  setup: string[];
  agent: string[];
  chart: Chart;
  /** The measure the chart draws (the first one when unset or not picked). */
  y: MeasureId | null;
}

export const DEFAULT_STATE: BuilderState = {
  measures: ['cost', 'runs'], dimensions: ['agent'], range: DEFAULT_RANGE, experiment: [], setup: [], agent: [], chart: 'bar', y: null,
};

export interface Preset { label: string; measures: MeasureId[]; dimensions: DimensionId[]; chart: Chart }
export const PRESETS: readonly Preset[] = [
  { label: 'Cost by agent', measures: ['cost', 'runs'], dimensions: ['agent'], chart: 'bar' },
  { label: 'Refusals by tool', measures: ['refusals', 'tool_calls'], dimensions: ['tool'], chart: 'bar' },
  { label: 'Tokens by model', measures: ['tokens_in', 'tokens_out', 'tokens_cached'], dimensions: ['model'], chart: 'bar' },
  { label: 'Pass rate by setup', measures: ['pass_rate', 'runs'], dimensions: ['setup'], chart: 'bar' },
  { label: 'Errors by step kind', measures: ['errors', 'steps'], dimensions: ['kind'], chart: 'bar' },
  { label: 'Cost per day by agent', measures: ['cost'], dimensions: ['day', 'agent'], chart: 'stacked' },
];

const KEYS = ['m', 'd', 'range', 'experiment', 'setup', 'agent', 'chart', 'y'] as const;
const known = <T extends string>(all: readonly T[], vs: string[]) => [...new Set(vs.filter((v): v is T => all.includes(v as T)))];
const picks = (sp: URLSearchParams, k: string) => [...new Set(sp.getAll(k).filter(Boolean))];

/** The builder state in `sp`; anything unknown falls back to the default. With no measure in the address, the default query. */
export function parseBuilder(sp: URLSearchParams): BuilderState {
  const measures = known(MEASURE_IDS, sp.getAll('m')).slice(0, MAX_MEASURES);
  const fresh = !measures.length;
  const range = RANGES.some((r) => r.id === sp.get('range')) ? (sp.get('range') as Range) : DEFAULT_RANGE;
  const chart = CHARTS.some((c) => c.id === sp.get('chart')) ? (sp.get('chart') as Chart) : DEFAULT_STATE.chart;
  const y = known(MEASURE_IDS, [sp.get('y') ?? ''])[0] ?? null;
  return {
    measures: fresh ? DEFAULT_STATE.measures : measures,
    dimensions: fresh ? DEFAULT_STATE.dimensions : known(DIMENSION_IDS, sp.getAll('d')).slice(0, MAX_DIMENSIONS),
    range, experiment: picks(sp, 'experiment'), setup: picks(sp, 'setup'), agent: picks(sp, 'agent'), chart, y,
  };
}

/** `prev` with `s` written in (the range and chart only when not the default; other keys kept). */
export function writeBuilder(prev: URLSearchParams, s: BuilderState): URLSearchParams {
  const sp = new URLSearchParams(prev);
  for (const k of KEYS) sp.delete(k);
  for (const m of s.measures) sp.append('m', m);
  for (const d of s.dimensions) sp.append('d', d);
  if (s.range !== DEFAULT_RANGE) sp.set('range', s.range);
  for (const k of ['experiment', 'setup', 'agent'] as const) for (const v of s[k]) sp.append(k, v);
  if (s.chart !== DEFAULT_STATE.chart) sp.set('chart', s.chart);
  if (s.y && s.measures.includes(s.y)) sp.set('y', s.y);
  return sp;
}

/** The measure the chart draws. */
export const chartedMeasure = (s: BuilderState): MeasureId => (s.y && s.measures.includes(s.y) ? s.y : s.measures[0]);

/** The API spec for `s` at `now` (the range made a start time; empty lists left out). */
export function apiSpec(s: BuilderState, now: Date): Spec {
  const filters: NonNullable<Spec['filters']> = {};
  const from = fromOf(s.range, now);
  if (from) filters.from = from;
  for (const k of ['experiment', 'setup', 'agent'] as const) if (s[k].length) filters[k] = s[k];
  return { measures: s.measures, dimensions: s.dimensions, filters };
}

/** What a saved builder view keeps: the spec without the start time (the range is kept instead, so "last 30 days" stays relative). */
export function savedSpec(s: BuilderState): BuilderSpec {
  const { filters } = apiSpec(s, new Date());
  delete filters?.from;
  return { measures: s.measures, dimensions: s.dimensions, filters, chart: s.chart, range: s.range, ...(s.y ? { y: s.y } : {}) };
}

/** The builder state of a saved view. */
export function fromSaved(v: BuilderSpec): BuilderState {
  const f = v.filters ?? {};
  return {
    measures: known(MEASURE_IDS, v.measures ?? []).slice(0, MAX_MEASURES),
    dimensions: known(DIMENSION_IDS, v.dimensions ?? []).slice(0, MAX_DIMENSIONS),
    range: v.range ?? (f.from ? 'all' : DEFAULT_RANGE),
    experiment: f.experiment ?? [], setup: f.setup ?? [], agent: f.agent ?? [],
    chart: v.chart ?? DEFAULT_STATE.chart, y: v.y ?? null,
  };
}

/** `s` with a preset's measures, dimensions and chart. */
export const withPreset = (s: BuilderState, p: Preset): BuilderState => ({ ...s, measures: [...p.measures], dimensions: [...p.dimensions], chart: p.chart, y: null });

/** `list` with `v` added or removed. */
export const toggled = <T>(list: T[], v: T): T[] => (list.includes(v) ? list.filter((x) => x !== v) : [...list, v]);
