// The Analytics charts' data and uPlot options, as pure functions: pass rate over time (one line
// per setup or model) and cost per run against pass rate (one dot per setup or model).
import type uPlot from 'uplot';
import type { GroupPoint, SeriesPoint } from '@/api/analytics';
import { usd } from '@/lib/format';
import type { ChartColors } from '@/lib/chart';

const DAY_S = 86_400;
const FONT = '11px Instrument Sans Variable, system-ui';

/** One line of the pass-rate chart: its group, and its rate (%) per bucket (null: nothing judged then). */
export interface Line { group: string; rates: (number | null)[]; counts: { passed: number; judged: number }[] }

/** The buckets (epoch seconds) and one line per group, groups in `order` (the busiest first). */
export function linesOf(series: SeriesPoint[], order: string[]): { xs: number[]; lines: Line[] } {
  const xs = [...new Set(series.map((p) => Date.parse(p.t) / 1000))].sort((a, b) => a - b);
  const at = new Map(xs.map((x, i) => [x, i]));
  const groups = order.filter((g) => series.some((p) => p.group === g));
  const lines = groups.map((group) => {
    const rates: (number | null)[] = xs.map(() => null);
    const counts = xs.map(() => ({ passed: 0, judged: 0 }));
    for (const p of series.filter((s) => s.group === group)) {
      const i = at.get(Date.parse(p.t) / 1000)!;
      rates[i] = p.pass_rate == null ? null : p.pass_rate * 100;
      counts[i] = { passed: p.passed, judged: p.judged };
    }
    return { group, rates, counts };
  });
  return { xs, lines };
}

/** "Nov 23": a bucket's day, in UTC like the buckets. */
export const dayLabel = (s: number) => new Date(s * 1000).toLocaleDateString(undefined, { month: 'short', day: 'numeric', timeZone: 'UTC' });

/** At most `n` of `xs`, evenly spread, first and last kept: the time axis' ticks. */
export function ticksOf(xs: number[], n = 6): number[] {
  if (xs.length <= n) return xs;
  const step = (xs.length - 1) / (n - 1);
  return Array.from({ length: n }, (_, i) => xs[Math.round(i * step)]);
}

function axis(c: ChartColors) {
  return { stroke: c.axis, grid: { stroke: c.grid, width: 1 }, ticks: { show: false }, font: FONT };
}

/** The pass-rate chart: a time axis over the buckets (padded half a bucket, so one bucket still has room), 0–100 %. */
export function lineOptions(width: number, height: number, c: ChartColors, xs: number[], lines: Line[], colors: string[], bucketS: number, onCursor: (idx: number | null, left: number) => void): uPlot.Options {
  // `left` is in the chart box's pixels, -1 once the cursor leaves.
  const sparse = xs.length <= 40;
  return {
    width, height, legend: { show: false },
    cursor: { y: false, drag: { x: false, y: false }, points: { size: 10, width: 2 } },
    select: { show: false, left: 0, top: 0, width: 0, height: 0 },
    scales: {
      x: { time: true, range: (_u, min, max) => [min - bucketS / 2, max + bucketS / 2] },
      y: { range: [0, 100] },
    },
    axes: [
      { ...axis(c), splits: () => ticksOf(xs), values: (_u, v) => v.map(dayLabel) },
      { ...axis(c), size: 44, splits: () => [0, 25, 50, 75, 100], values: (_u, v) => v.map((n) => `${n}%`) },
    ],
    series: [
      {},
      ...lines.map((l, i) => ({
        label: l.group, stroke: colors[i], width: 2, spanGaps: true,
        points: { show: sparse, size: 8, width: 2, fill: colors[i], stroke: c.surface },
      })),
    ],
    hooks: { setCursor: [(u: uPlot) => onCursor(u.cursor.idx ?? null, (u.cursor.left ?? -1) < 0 ? -1 : u.over.offsetLeft + (u.cursor.left ?? 0))] },
  };
}

export const bucketSeconds = (bucket: 'day' | 'week') => (bucket === 'week' ? 7 * DAY_S : DAY_S);

/** A dot of the cost chart: a group with a known cost per run and pass rate. */
export interface Dot { group: string; x: number; y: number; size: number; point: GroupPoint }

/** The dots, cheapest first (uPlot wants its x ascending), sized by runs (area ∝ runs, 8–28 px); and the groups that can't be placed. */
export function dotsOf(points: GroupPoint[]): { dots: Dot[]; unplaced: GroupPoint[] } {
  const placed = points.filter((p) => p.cost_per_run != null && p.pass_rate != null);
  const most = Math.max(1, ...placed.map((p) => p.runs));
  const dots = placed
    .map((p) => ({ group: p.group, x: p.cost_per_run!, y: p.pass_rate! * 100, size: 8 + 20 * Math.sqrt(p.runs / most), point: p }))
    .sort((a, b) => a.x - b.x || a.group.localeCompare(b.group));
  return { dots, unplaced: points.filter((p) => !placed.includes(p)) };
}

/** uPlot's aligned data for `dots`: one x per dot, each dot its own series with a value at its x only. */
export function scatterData(dots: Dot[]): uPlot.AlignedData {
  const xs = dots.map((d) => d.x);
  return [xs, ...dots.map((d, i) => xs.map((_, j) => (j === i ? d.y : null)))];
}

/** The cost chart: cost per run from $0 (room past the dearest), pass rate 0–100 %, dots only, no crosshair. */
export function scatterOptions(width: number, height: number, c: ChartColors, dots: Dot[], colors: string[], hooks: uPlot.Hooks.Arrays): uPlot.Options {
  const most = Math.max(0, ...dots.map((d) => d.x));
  return {
    width, height, legend: { show: false },
    cursor: { x: false, y: false, drag: { x: false, y: false }, points: { show: false } },
    select: { show: false, left: 0, top: 0, width: 0, height: 0 },
    scales: { x: { time: false, range: () => [0, most > 0 ? most * 1.15 : 1] }, y: { range: [0, 100] } },
    axes: [
      { ...axis(c), values: (_u, v) => v.map((n) => usd(n)) },
      { ...axis(c), size: 44, splits: () => [0, 25, 50, 75, 100], values: (_u, v) => v.map((n) => `${n}%`) },
    ],
    series: [
      {},
      ...dots.map((d, i) => ({
        label: d.group, stroke: c.surface, width: 0, paths: () => null,
        points: { show: true, size: d.size, width: 2, fill: colors[i], stroke: c.surface },
      })),
    ],
    hooks,
  };
}

/** The dot under (`left`, `top`) in the plot's pixels, given each dot's centre: within its radius plus a margin. */
export function dotAt(centres: { x: number; y: number; r: number }[], left: number, top: number): number | null {
  let best: number | null = null;
  let bestD = Infinity;
  centres.forEach((c, i) => {
    const d = Math.hypot(c.x - left, c.y - top);
    if (d <= c.r + 6 && d < bestD) { best = i; bestD = d; }
  });
  return best;
}
