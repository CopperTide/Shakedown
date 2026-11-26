import { useCallback, useMemo, useRef, useState } from 'react';
import type uPlot from 'uplot';
import type { Group, Summary } from '@/api/analytics';
import { type Make, usePlot } from '@/components/ui/chart';
import { Segmented } from '@/components/ui/segmented';
import { colorOf, resolved, SLOTS } from './colors';
import { bucketSeconds, dayLabel, type Line, lineOptions, linesOf } from './charts';
import { Card, Legend, Tip, pct } from './parts';

const HEIGHT = 220;
export const GROUPS: readonly { id: Group; label: string }[] = [{ id: 'setup', label: 'Setup' }, { id: 'model', label: 'Model' }];

/** Each line's pass rate in the bucket under the cursor, with its passes of judged runs. */
function LinesTip({ lines, colors, idx, left, label }: { lines: Line[]; colors: string[]; idx: number; left: number; label: string }) {
  return (
    <Tip left={left}>
      <p className="mb-1 font-medium">{label}</p>
      {lines.map((l, i) => {
        const n = l.counts[idx];
        if (!n.judged) return null;
        return (
          <p key={l.group} className="flex items-center gap-1.5 tabular-nums">
            <span aria-hidden="true" className="size-2 shrink-0 rounded-full" style={{ background: colors[i] }} />
            <span className="min-w-0 flex-1 truncate">{l.group}</span>
            <span className="font-medium">{pct(n.passed / n.judged)}</span>
            <span className="text-muted">{n.passed}/{n.judged}</span>
          </p>
        );
      })}
    </Tip>
  );
}

/** Pass rate per day or week, one line per setup (or model): the busiest five, a dot per bucket while they are few. */
export default function PassRateChart({ summary, onGroup }: { summary: Summary; onGroup: (g: Group) => void }) {
  const names = summary.options[summary.group];
  const order = summary.points.map((p) => p.group);
  const { xs, lines } = useMemo(() => linesOf(summary.series, order.slice(0, SLOTS)), [summary]); // eslint-disable-line react-hooks/exhaustive-deps
  const colors = lines.map((l) => colorOf(l.group, names));
  const hidden = order.length - lines.length;
  const [hover, setHover] = useState<{ idx: number; left: number } | null>(null);
  const onCursor = useRef((idx: number | null, left: number) => setHover(idx == null || left < 0 ? null : { idx, left }));
  const data = useMemo<uPlot.AlignedData>(() => [xs, ...lines.map((l) => l.rates)], [xs, lines]);
  const make = useCallback<Make>(
    (w, c) => lineOptions(w, HEIGHT, c, xs, lines, colors.map((col) => resolved(col)), bucketSeconds(summary.bucket), (i, l) => onCursor.current(i, l)),
    [xs, lines, colors.join(), summary.bucket], // eslint-disable-line react-hooks/exhaustive-deps
  );
  const box = usePlot(data, make);
  const per = summary.bucket === 'week' ? 'week' : 'day';
  return (
    <Card title="Pass rate over time" hint={`Runs that passed, of those judged, per ${per} (UTC). Stopped runs and platform failures are left out.`}
      actions={<Segmented label="One line per" value={summary.group} options={GROUPS} onChange={onGroup} />}>
      <div className="relative">
        <div ref={box} role="img" aria-label={`Pass rate per ${per}, one line per ${summary.group}`} className="w-full" style={{ height: HEIGHT }} />
        {hover && <LinesTip lines={lines} colors={colors} idx={hover.idx} left={hover.left} label={`${per === 'week' ? 'Week of ' : ''}${dayLabel(xs[hover.idx])}`} />}
      </div>
      <div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1">
        <Legend items={lines.map((l, i) => ({ name: l.group, color: colors[i] }))} />
        {hidden > 0 && <span className="text-xs text-faint">+{hidden} more with fewer runs: pick them in the filter to see them</span>}
      </div>
    </Card>
  );
}
