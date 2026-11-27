// A result shaped for a chart: categories along x (the first dimension, or an SQL column), one
// series per value of the second dimension (or per numeric SQL column), the raw keys kept so a bar
// drills through to the rows it stands for. Pure functions only.
import type { Cell, QueryResult, SqlResult, Unit } from '@/api/explore';
import { dimensionText, sqlText } from './values';

/** The most categories and series a chart draws; the table keeps every row. */
export const MAX_X = 40;
export const MAX_SERIES = 8;

export interface Series { name: string; key: Cell; values: (number | null)[] }
export interface ChartData {
  labels: string[];
  /** Each category's raw value (what a drill sends). */
  keys: Cell[];
  series: Series[];
  unit?: Unit;
  /** Categories and series left out of the chart. */
  hidden: { x: number; series: number };
}

const num = (v: unknown): number | null => {
  const n = typeof v === 'number' ? v : typeof v === 'string' && v.trim() !== '' ? Number(v) : NaN;
  return Number.isFinite(n) ? n : null;
};

/** Distinct values in first-seen order. */
function distinct(vs: Cell[]): Cell[] {
  const seen = new Set<string>();
  return vs.filter((v) => {
    const k = JSON.stringify(v);
    return seen.has(k) ? false : (seen.add(k), true);
  });
}

/** A builder result as a chart of measure `y`: no dimension → one bar; one → a series; two → a series per second-dimension value (largest first). */
export function builderChart(r: QueryResult, y: string): ChartData {
  // The measure asked for, else the first (a previous result can still be on screen while the next loads).
  const found = r.columns.findIndex((c) => c.id === y);
  const yi = found >= 0 ? found : r.columns.findIndex((c) => c.role === 'measure');
  const dims = r.columns.filter((c) => c.role === 'dimension');
  const unit = r.columns[yi]?.unit;
  const label = (v: Cell, d: number) => dimensionText(v, dims[d].id, r.labels);
  if (!dims.length) {
    return { labels: ['All'], keys: [null], series: [{ name: r.columns[yi]?.label ?? y, key: null, values: [num(r.rows[0]?.[yi])] }], unit, hidden: { x: 0, series: 0 } };
  }
  const allKeys = distinct(r.rows.map((row) => row[0]));
  const keys = allKeys.slice(0, MAX_X);
  if (dims.length === 1) {
    const at = new Map(r.rows.map((row) => [JSON.stringify(row[0]), num(row[yi])]));
    const values = keys.map((k) => at.get(JSON.stringify(k)) ?? null);
    return { labels: keys.map((k) => label(k, 0)), keys, series: [{ name: r.columns[yi].label, key: null, values }], unit, hidden: { x: allKeys.length - keys.length, series: 0 } };
  }
  const { series, hidden } = pivot(r.rows, keys, yi);
  return { labels: keys.map((k) => label(k, 0)), keys, series: series.map((s) => ({ ...s, name: label(s.key, 1) })), unit, hidden: { x: allKeys.length - keys.length, series: hidden } };
}

/** One series per distinct second-column value over `keys`, the biggest totals first, at most MAX_SERIES. */
function pivot(rows: Cell[][], keys: Cell[], yi: number): { series: Series[]; hidden: number } {
  const xAt = new Map(keys.map((k, i) => [JSON.stringify(k), i]));
  const bySeries = new Map<string, Series>();
  for (const row of rows) {
    const x = xAt.get(JSON.stringify(row[0]));
    if (x == null) continue;
    const sk = JSON.stringify(row[1]);
    if (!bySeries.has(sk)) bySeries.set(sk, { name: '', key: row[1], values: keys.map(() => null) });
    bySeries.get(sk)!.values[x] = num(row[yi]);
  }
  const total = (s: Series) => s.values.reduce<number>((n, v) => n + (v ?? 0), 0);
  const all = [...bySeries.values()].sort((a, b) => total(b) - total(a));
  return { series: all.slice(0, MAX_SERIES), hidden: Math.max(0, all.length - MAX_SERIES) };
}

/** An SQL result as a chart: column `x` along the axis, one series per column in `ys` (its numbers). */
export function sqlChart(r: SqlResult, x: string, ys: string[]): ChartData {
  const xi = r.columns.findIndex((c) => c.name === x);
  const rows = r.rows.slice(0, MAX_X);
  const series = ys.slice(0, MAX_SERIES).map((y) => {
    const yi = r.columns.findIndex((c) => c.name === y);
    return { name: y, key: y, values: rows.map((row) => num(row[yi])) };
  });
  const keys = rows.map((row) => (xi < 0 ? null : (row[xi] as Cell)));
  return { labels: rows.map((row) => (xi < 0 ? '' : sqlText(row[xi]))), keys, series, hidden: { x: r.rows.length - rows.length, series: Math.max(0, ys.length - MAX_SERIES) } };
}

/** The value axis' top: the largest value (stacked: the largest category total), never 0. */
export function topOf(d: ChartData, stacked: boolean): number {
  const per = d.labels.map((_, i) => d.series.map((s) => s.values[i] ?? 0));
  const top = Math.max(0, ...per.map((vs) => (stacked ? vs.reduce((a, b) => a + Math.max(0, b), 0) : Math.max(0, ...vs))));
  return top > 0 ? top : 1;
}

/** Each series' bottom in each category when stacked: the sum of the series below it. */
export function stackBases(d: ChartData): number[][] {
  const run = d.labels.map(() => 0);
  return d.series.map((s) => s.values.map((v, i) => {
    const base = run[i];
    run[i] += Math.max(0, v ?? 0);
    return base;
  }));
}

/** The default SQL chart: the first non-numeric column along x (else the first), the numeric others as series. */
export function defaultAxes(r: SqlResult): { x: string; ys: string[] } {
  const x = (r.columns.find((c) => c.kind !== 'number') ?? r.columns[0])?.name ?? '';
  return { x, ys: r.columns.filter((c) => c.kind === 'number' && c.name !== x).map((c) => c.name) };
}
