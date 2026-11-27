// Explore's values as text: a measure by its unit, a dimension value by its label (an experiment's
// name, "(none)" for no value), any SQL value, and a result as CSV. Pure functions only.
import type { Cell, Unit } from '@/api/explore';
import { duration, usd } from '@/lib/format';

const compact = new Intl.NumberFormat(undefined, { notation: 'compact', maximumFractionDigits: 1 });

/** A measure's value: "$0.56", "67%", "3.1 min", "12.4K", "1,204"; "–" for none. */
export function measureText(v: Cell, unit: Unit | undefined): string {
  if (v == null || v === '') return '–';
  const n = typeof v === 'number' ? v : Number(v);
  if (!Number.isFinite(n)) return String(v);
  switch (unit) {
    case 'usd': return usd(n);
    case 'ratio': return `${Math.round(n * 100)}%`;
    case 'seconds': return duration(n);
    case 'tokens': return n >= 10_000 ? compact.format(n) : Math.round(n).toLocaleString();
    default: return Number.isInteger(n) ? n.toLocaleString() : n.toFixed(2);
  }
}

/** A short axis value: "$1.2", "50%", "12K". */
export function axisText(n: number, unit: Unit | undefined): string {
  if (unit === 'ratio') return `${Math.round(n * 100)}%`;
  if (unit === 'seconds') return duration(n);
  return `${unit === 'usd' ? '$' : ''}${compact.format(n)}`;
}

export const NONE = '(none)';

/** A dimension value as shown: an experiment by its name, nothing as "(none)". */
export function dimensionText(v: Cell, id: string, labels: { experiment?: Record<string, string> }): string {
  if (v == null || v === '') return NONE;
  if (id === 'experiment') return labels.experiment?.[String(v)] ?? String(v);
  return String(v);
}

/** Any SQL value in one line: text as it is, JSON compact, nothing as empty. */
export function sqlText(v: unknown): string {
  if (v == null) return '';
  return typeof v === 'object' ? JSON.stringify(v) : String(v);
}

/** One CSV field: quoted when it holds a comma, quote, or line break (quotes doubled). */
export function csvField(v: unknown): string {
  const s = sqlText(v);
  return /[",\r\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

/** A header row and rows as CSV (RFC 4180 line ends). */
export const toCsv = (header: string[], rows: unknown[][]) =>
  [header, ...rows].map((r) => r.map(csvField).join(',')).join('\r\n') + '\r\n';

/** Hands the browser `text` as a file named `name`. */
export function download(name: string, text: string, type = 'text/csv;charset=utf-8') {
  const url = URL.createObjectURL(new Blob([text], { type }));
  const a = Object.assign(document.createElement('a'), { href: url, download: name });
  document.body.append(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}
