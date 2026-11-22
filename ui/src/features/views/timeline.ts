// A timeline's geometry: rows by lane, the UTC axis's ticks, and where a time falls.
import { toMs, utcTime } from './format';
import { cols } from './rows';
import { type Tone, type TimelineData, asTone } from './types';

export interface Bar {
  key: string;
  label: string;
  start: number;
  end: number;
  tone: Tone;
  chip: string;
}

/** Each lane's bars, in start order; rows without a lane or a start are left out. */
export function barsByLane(data: TimelineData): Bar[][] {
  const c = cols(data.columns);
  const lanes: Bar[][] = data.lanes.map(() => []);
  for (const row of data.rows) {
    const lane = c.num(row, 'lane');
    const start = toMs(c.get(row, 'start'));
    if (lane == null || start == null || !lanes[lane]) continue;
    const end = toMs(c.get(row, 'end')) ?? start;
    lanes[lane].push({ key: c.str(row, 'key'), label: c.str(row, 'label'), start, end: Math.max(start, end), tone: asTone(c.get(row, 'tone')), chip: c.str(row, 'chip') });
  }
  for (const l of lanes) l.sort((a, b) => a.start - b.start);
  return lanes;
}

/** The time range drawn: the payload's when it is a real interval, else the bars' extent (an hour when empty). */
export function rangeOf(data: TimelineData, lanes: Bar[][], now: number | null): [number, number] {
  const [a, b] = data.range ?? [0, 0];
  if (Number.isFinite(a) && Number.isFinite(b) && b > a) return [a, b];
  let lo = Infinity, hi = -Infinity;
  for (const l of lanes) for (const bar of l) { lo = Math.min(lo, bar.start); hi = Math.max(hi, bar.end); }
  if (!(hi > lo)) { const t = now ?? 0; return [t - 1_800_000, t + 1_800_000]; }
  return [lo, hi];
}

const MIN = 60_000;
const STEPS = [5, 10, 15, 30, 60, 120, 180, 360, 720, 1440].map((m) => m * MIN);

export interface Tick {
  ms: number;
  label: string;
  /** A tick on a whole hour (ruled stronger). */
  major: boolean;
}

/** Ticks on round UTC times across `[t0, t1]`, at least `minGap` px apart on a `width` px axis. */
export function ticks(t0: number, t1: number, width: number, minGap = 64): Tick[] {
  if (!(t1 > t0) || width <= 0) return [];
  const perPx = (t1 - t0) / width;
  const step = STEPS.find((s) => s / perPx >= minGap) ?? STEPS[STEPS.length - 1];
  const out: Tick[] = [];
  for (let t = Math.ceil(t0 / step) * step; t <= t1; t += step) {
    out.push({ ms: t, label: utcTime(t).replace('Z', ''), major: t % (60 * MIN) === 0 });
  }
  return out;
}

/** A time's x on an axis `width` px wide across `[t0, t1]`. */
export const xOf = (ms: number, t0: number, t1: number, width: number) => (t1 > t0 ? ((ms - t0) / (t1 - t0)) * width : 0);

export const ZOOMS = [1, 1.5, 2, 3, 4, 6, 8, 12, 16];

/** The next zoom step in or out. */
export function zoomStep(z: number, dir: 1 | -1): number {
  const i = ZOOMS.findIndex((x) => x >= z - 1e-9);
  const at = i < 0 ? ZOOMS.length - 1 : i;
  return ZOOMS[Math.min(ZOOMS.length - 1, Math.max(0, at + dir))];
}
