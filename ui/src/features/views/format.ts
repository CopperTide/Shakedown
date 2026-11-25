// A view's value formats: what a number or a time reads as on screen. Times are the world's local time, in
// its zone (`Zone`); a world that gives no zone is shown in UTC ("14:05Z").
import { toNum } from './rows';
import type { Format } from './types';

export const DASH = '–';

const pad = (n: number) => String(n).padStart(2, '0');
const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
const DAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];

/** Epoch ms (or an ISO time) as epoch ms; null when it is neither. */
export function toMs(v: unknown): number | null {
  const n = toNum(v);
  if (n != null) return n;
  if (typeof v === 'string') {
    const t = Date.parse(v);
    return Number.isNaN(t) ? null : t;
  }
  return null;
}

/**
 * The world's time zone as its views show it: its UTC offset in minutes and its label ("UTC−4"). A world that
 * gives no offset (its `clock_start` has none) is shown in UTC, its times marked "Z".
 */
export interface Zone {
  offsetMin: number;
  label: string;
}

export const UTC: Zone = { offsetMin: 0, label: 'Z' };

/** The zone of a world whose offset is `offsetMin` (`at.utc_offset_min`); UTC ("Z") without one. */
export function zoneOf(offsetMin: number | null | undefined): Zone {
  if (offsetMin == null || !Number.isFinite(offsetMin)) return UTC;
  const m = Math.abs(offsetMin), h = Math.floor(m / 60), rest = m % 60;
  const sign = offsetMin < 0 ? '−' : '+';
  return { offsetMin, label: m === 0 ? 'UTC' : `UTC${sign}${h}${rest ? `:${pad(rest)}` : ''}` };
}

/** `ms` as the world's wall clock: a Date whose UTC fields read the zone's local time. */
const local = (ms: number, zone: Zone) => new Date(ms + zone.offsetMin * 60_000);

/** "10:05" in the world's zone ("14:05Z" in UTC), with seconds when asked. */
export function timeIn(ms: number, zone: Zone = UTC, seconds = false): string {
  const d = local(ms, zone);
  const hm = `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}${seconds ? `:${pad(d.getUTCSeconds())}` : ''}`;
  return zone === UTC ? `${hm}Z` : hm;
}

/** "Sun 22 Nov 2026", the world's local date. */
export function dateIn(ms: number, zone: Zone = UTC): string {
  const d = local(ms, zone);
  return `${DAYS[d.getUTCDay()]} ${d.getUTCDate()} ${MONTHS[d.getUTCMonth()]} ${d.getUTCFullYear()}`;
}

/** 75 → "1:15", -20 → "-0:20". */
export function minutes(v: number): string {
  const m = Math.round(Math.abs(v));
  return `${v < 0 && m > 0 ? '-' : ''}${Math.floor(m / 60)}:${pad(m % 60)}`;
}

const grouped = (n: number, decimals: number) =>
  n.toLocaleString('en-US', { minimumFractionDigits: decimals, maximumFractionDigits: decimals });

function usd(n: number, decimals?: number): string {
  const d = decimals ?? (Math.abs(n) >= 1000 ? 0 : 2);
  return `${n < 0 ? '-' : ''}$${grouped(Math.abs(n), d)}`;
}

/** A number in a format (a time in `zone`); null when the format isn't numeric. */
function numeric(n: number, format: Format, decimals?: number, zone?: Zone): string | null {
  switch (format) {
    case 'int': return grouped(Math.round(n), 0);
    case 'number': return grouped(n, decimals ?? (Number.isInteger(n) ? 0 : 1));
    case 'percent': return `${grouped(n, decimals ?? 1)}%`;
    case 'ratio_pct': return `${grouped(n * 100, decimals ?? 0)}%`;
    case 'usd': return usd(n, decimals);
    case 'minutes': return minutes(n);
    case 'time': return timeIn(n, zone);
    default: return null;
  }
}

function textOf(v: unknown, decimals?: number): string {
  if (typeof v === 'number' && decimals != null) return v.toFixed(decimals);
  return typeof v === 'object' ? JSON.stringify(v) : String(v);
}

/**
 * A value as a view's `format` shows it; `chip` shows the chip label, else the value; a time reads in the world's
 * `zone`. Nothing (null or absent) reads as a dash; a number format given text that isn't a number shows the text
 * as it is.
 */
export function formatValue(v: unknown, format: Format = 'text', decimals?: number, chip?: string | null, zone?: Zone): string {
  if (format === 'chip') return chip || (v == null || v === '' ? DASH : String(v));
  if (v == null || v === '') return DASH;
  if (format === 'text') return textOf(v, decimals);
  const n = format === 'time' ? toMs(v) : toNum(v);
  if (n == null) return String(v);
  return numeric(n, format, decimals, zone) ?? String(v);
}

/** Whether a column's values read as numbers (right-aligned, sorted as numbers). */
export const isNumericFormat = (f: Format | undefined) => f != null && f !== 'text' && f !== 'chip';
