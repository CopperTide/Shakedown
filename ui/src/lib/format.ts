// Values as text: seconds, tokens, dollars and plain counts, one way everywhere; anything else as JSON.

export type Unit = 's' | 'tok' | 'usd' | 'n';

/**
 * Dollars, one rule everywhere: cents from ten cents up ("$0.56"), two significant digits below
 * ("$0.041", "$0.0005"), "<$0.0001" for less, "$0.00" for none.
 */
export function usd(x: number): string {
  if (x === 0) return '$0.00';
  if (x >= 0.1) return `$${x.toFixed(2)}`;
  if (x < 0.0001) return '<$0.0001';
  return `$${Number(x.toPrecision(2))}`;
}

/** "3.0 s", "12.4k", "$0.56" (see `usd`), "2". */
export function fmt(x: number | null, unit: Unit): string {
  if (x == null) return '–';
  if (unit === 's') return `${x.toFixed(1)} s`;
  if (unit === 'usd') return usd(x);
  if (unit === 'tok') return x >= 10_000 ? `${(x / 1000).toFixed(1)}k` : `${Math.round(x)}`;
  return Number.isInteger(x) ? `${x}` : x.toFixed(2);
}

/** "45 s" under a minute and a half, else "3.1 min". */
export function duration(s: number | null): string {
  if (s == null) return '–';
  return s < 90 ? `${Math.round(s)} s` : `${(s / 60).toFixed(1)} min`;
}

/** A string as it is; anything else as indented JSON (`null` for nothing). */
export const pretty = (v: unknown) => (typeof v === 'string' ? v : JSON.stringify(v ?? null, null, 2));
