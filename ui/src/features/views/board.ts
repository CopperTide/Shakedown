// A board's rows: its columns as the file names them, sorted, quick-filtered and searched.
import { DASH, UTC, type Zone, formatValue, isNumericFormat, toMs } from './format';
import { type Cols, cols, columnOf, toNum } from './rows';
import type { BoardData, BoardDef, Field, Format } from './types';

export interface BoardColumn {
  /** The payload column it reads, or null when core sent none. */
  name: string | null;
  label: string;
  format: Format;
  decimals?: number;
  numeric: boolean;
}

export function boardColumns(def: BoardDef, c: Cols): BoardColumn[] {
  return (def.columns ?? []).map((col) => {
    const format = col.format ?? 'text';
    return { name: columnOf(c, col.field), label: col.label, format, decimals: col.decimals, numeric: isNumericFormat(format) && format !== 'time' };
  });
}

/** A cell's text as shown: chip columns show the row's chip (else the value); times read in the world's `zone`. */
export function cellText(c: Cols, row: unknown[], col: BoardColumn, zone: Zone = UTC): string {
  if (col.name == null) return DASH;
  const chip = col.format === 'chip' ? (c.get(row, `${col.name}:chip`) as string | undefined) ?? c.str(row, 'chip') : null;
  return formatValue(c.get(row, col.name), col.format, col.decimals, chip, zone);
}

export interface Sort {
  column: number;
  desc: boolean;
}

/** The def's default sort as a column index; the first column ascending when it names none on screen. */
export function defaultSort(def: BoardDef, columns: BoardColumn[], c: Cols): Sort | null {
  if (!def.sort) return null;
  const name = columnOf(c, def.sort.field as Field);
  const i = columns.findIndex((col) => col.name != null && col.name === name);
  return i < 0 ? null : { column: i, desc: !!def.sort.desc };
}

function sortKey(v: unknown, col: BoardColumn): number | string | null {
  if (v == null || v === '') return null;
  if (col.format === 'time') return toMs(v);
  if (col.numeric) return toNum(v);
  return typeof v === 'number' ? v : String(v).toLowerCase();
}

function compare(a: number | string | null, b: number | string | null): number {
  if (a == null || b == null) return a == null ? (b == null ? 0 : 1) : -1;
  if (typeof a === 'number' && typeof b === 'number') return a - b;
  return String(a).localeCompare(String(b), 'en', { numeric: true });
}

export interface BoardQuery {
  sort: Sort | null;
  /** Indexes into the def's filters that are on: a row shows when it matches any of them. */
  filters: number[];
  search: string;
  /** The zone times are searched in (as they are shown). */
  zone?: Zone;
}

/** A row's filter bits (bit i: the def's filters[i] matched). */
export const filterBits = (c: Cols, row: unknown[]) => toNum(c.get(row, 'filters')) ?? 0;

function matchesSearch(c: Cols, row: unknown[], columns: BoardColumn[], needle: string, zone: Zone): boolean {
  if (!needle) return true;
  if (c.str(row, 'key').toLowerCase().includes(needle) || c.str(row, 'chip').toLowerCase().includes(needle)) return true;
  return columns.some((col) => !col.numeric && col.name != null && cellText(c, row, col, zone).toLowerCase().includes(needle));
}

/** The indexes of the rows to show, in order. */
export function boardRows(data: BoardData, columns: BoardColumn[], q: BoardQuery): number[] {
  const c = cols(data.columns);
  const needle = q.search.trim().toLowerCase();
  const mask = q.filters.reduce((m, i) => m | (1 << i), 0);
  const out: number[] = [];
  data.rows.forEach((row, i) => {
    if (mask && !(filterBits(c, row) & mask)) return;
    if (matchesSearch(c, row, columns, needle, q.zone ?? UTC)) out.push(i);
  });
  const col = q.sort ? columns[q.sort.column] : undefined;
  if (q.sort && col?.name) {
    const name = col.name, sign = q.sort.desc ? -1 : 1;
    const keys = new Map(out.map((i) => [i, sortKey(c.get(data.rows[i], name), col)]));
    out.sort((a, b) => {
      const ka = keys.get(a) ?? null, kb = keys.get(b) ?? null;
      if (ka == null || kb == null) return compare(ka, kb);
      return sign * compare(ka, kb) || a - b;
    });
  }
  return out;
}

/** How many rows match each filter. */
export function filterCounts(data: BoardData, n: number): number[] {
  const c = cols(data.columns);
  const counts = Array.from({ length: n }, () => 0);
  for (const row of data.rows) {
    const bits = filterBits(c, row);
    for (let i = 0; i < n; i++) if (bits & (1 << i)) counts[i]++;
  }
  return counts;
}

/** The next sort after a click on a column's header: ascending, then descending, then the default. */
export function nextSort(cur: Sort | null, column: number, numeric: boolean): Sort | null {
  if (!cur || cur.column !== column) return { column, desc: numeric };
  if (cur.desc === numeric) return { column, desc: !numeric };
  return null;
}

/** A width per column in `ch`: the longest of its label and the first rows' cells, within bounds. */
export function columnWidths(data: BoardData, columns: BoardColumn[], sample = 300): number[] {
  const c = cols(data.columns);
  const rows = data.rows.slice(0, sample);
  return columns.map((col) => {
    let w = col.label.length + 2;
    for (const r of rows) w = Math.max(w, cellText(c, r, col).length + (col.format === 'chip' ? 2 : 0));
    return Math.min(40, Math.max(4, w));
  });
}
