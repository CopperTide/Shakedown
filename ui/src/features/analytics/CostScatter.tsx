import { useCallback, useMemo, useRef, useState } from 'react';
import type uPlot from 'uplot';
import type { GroupPoint, Summary } from '@/api/analytics';
import { type Make, usePlot } from '@/components/ui/chart';
import { fmt } from '@/lib/format';
import { colorOf, resolved } from './colors';
import { type Dot, dotAt, dotsOf, scatterData, scatterOptions } from './charts';
import { Card, Legend, Tip, money, pct } from './parts';

const HEIGHT = 220;

/** The plot of `dots`, the dot under the cursor (with where to show its tooltip), and a click opening a dot's runs. */
function useScatter(dots: Dot[], colors: string[], onOpen: (group: string) => void) {
  const [hover, setHover] = useState<{ i: number; left: number; top: number } | null>(null);
  const live = useRef({ dots, onOpen, hovered: null as number | null });
  live.current = { ...live.current, dots, onOpen };
  const data = useMemo(() => scatterData(dots), [dots]);
  const make = useCallback<Make>((w, c) => {
    const pick = (u: uPlot) => {
      const { left = -1, top = -1 } = u.cursor;
      if (left < 0) return null;
      const centres = live.current.dots.map((d) => ({ x: u.valToPos(d.x, 'x'), y: u.valToPos(d.y, 'y'), r: d.size / 2 }));
      return dotAt(centres, left, top);
    };
    const hooks: uPlot.Hooks.Arrays = {
      setCursor: [(u) => {
        const i = pick(u);
        live.current.hovered = i;
        u.over.style.cursor = i == null ? 'default' : 'pointer';
        setHover(i == null ? null : { i, left: u.over.offsetLeft + (u.cursor.left ?? 0), top: u.over.offsetTop + (u.cursor.top ?? 0) });
      }],
      ready: [(u) => u.over.addEventListener('click', () => {
        const i = live.current.hovered;
        if (i != null) live.current.onOpen(live.current.dots[i].group);
      })],
    };
    return scatterOptions(w, HEIGHT, c, dots, colors.map((col) => resolved(col)), hooks);
  }, [dots, colors.join()]); // eslint-disable-line react-hooks/exhaustive-deps
  const box = usePlot(data, make);
  return { box, hover };
}

/** Cost per run against pass rate, one dot per setup (or model), sized by its runs; a click opens its runs. */
export default function CostScatter({ summary, onOpen }: { summary: Summary; onOpen: (group: string) => void }) {
  const names = summary.options[summary.group];
  const { dots, unplaced } = useMemo(() => dotsOf(summary.points), [summary.points]);
  const colors = dots.map((d) => colorOf(d.group, names));
  const { box, hover } = useScatter(dots, colors, onOpen);
  const dot = hover ? dots[hover.i] : null;
  return (
    <Card title="Cost against pass rate" hint={`Each dot is a ${summary.group}: up and to the left is better. Bigger dots have more runs. Click one to see its runs.`}>
      <div className="relative">
        <div ref={box} role="img" aria-label={`Cost per run against pass rate, one dot per ${summary.group}`} className="w-full" style={{ height: HEIGHT }} />
        {dot && hover && <Tip left={hover.left} top={Math.max(0, hover.top - 24)}><DotTip point={dot.point} /></Tip>}
      </div>
      <div className="mt-2 flex flex-col gap-1">
        <Legend items={dots.map((d, i) => ({ name: d.group, color: colors[i] }))} />
        {unplaced.length > 0 && (
          <p className="text-xs text-faint">Not drawn (no cost known, or no judged run): {unplaced.map((p) => p.group).join(', ')}</p>
        )}
      </div>
    </Card>
  );
}

function DotTip({ point: p }: { point: GroupPoint }) {
  return (
    <>
      <p className="mb-1 font-medium break-words">{p.group}</p>
      <dl className="grid grid-cols-[auto_auto] gap-x-3 tabular-nums">
        <dt className="text-muted">Pass rate</dt><dd>{pct(p.pass_rate)} ({p.passed}/{p.judged})</dd>
        <dt className="text-muted">Cost per run</dt><dd>{money(p.cost_per_run)}</dd>
        <dt className="text-muted">Runs</dt><dd>{p.runs}</dd>
        <dt className="text-muted">Tokens</dt><dd>{fmt(p.tokens, 'tok')}</dd>
      </dl>
    </>
  );
}
