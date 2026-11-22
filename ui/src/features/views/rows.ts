// Reading core's compact rows by column name, never by position: core may add columns.
import type { Field, Rows } from './types';

export interface Cols {
  /** The index of a column, or -1. */
  index: (name: string) => number;
  has: (name: string) => boolean;
  /** A row's value under a column (undefined when the column is absent). */
  get: (row: unknown[], name: string) => unknown;
  num: (row: unknown[], name: string) => number | null;
  str: (row: unknown[], name: string) => string;
}

export function cols(columns: readonly string[] | undefined): Cols {
  const at = new Map<string, number>();
  (columns ?? []).forEach((c, i) => { if (!at.has(c)) at.set(c, i); });
  const index = (name: string) => at.get(name) ?? -1;
  const get = (row: unknown[], name: string) => {
    const i = index(name);
    return i < 0 ? undefined : row[i];
  };
  return {
    index,
    has: (name) => at.has(name),
    get,
    num: (row, name) => toNum(get(row, name)),
    str: (row, name) => { const v = get(row, name); return v == null ? '' : String(v); },
  };
}

export function toNum(v: unknown): number | null {
  if (typeof v === 'number') return Number.isFinite(v) ? v : null;
  if (typeof v === 'string' && v.trim() !== '' && Number.isFinite(Number(v))) return Number(v);
  return null;
}

/** The names a field may come back under: its dotted path; for a list, the list joined with `|`, then each path. */
export function fieldNames(f: Field): string[] {
  if (!Array.isArray(f)) return [f];
  return [f.join('|'), ...f];
}

/** The column a field comes back under in `columns`, or null when core sent none for it. */
export function columnOf(c: Cols, f: Field): string | null {
  return fieldNames(f).find((n) => c.has(n)) ?? null;
}

export const emptyRows = (r: Partial<Rows> | null | undefined): Rows => ({ columns: r?.columns ?? [], rows: r?.rows ?? [] });
