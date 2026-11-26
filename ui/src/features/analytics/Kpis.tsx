import type { ReactNode } from 'react';
import type { Totals } from '@/api/analytics';
import { duration } from '@/lib/format';
import { money, pct } from './parts';

function Tile({ label, value, note }: { label: string; value: ReactNode; note?: ReactNode }) {
  return (
    <div className="flex min-w-0 flex-col gap-1 rounded-lg border border-line bg-surface px-4 py-3 shadow-panel">
      <span className="text-xs font-medium text-muted">{label}</span>
      <span className="font-display text-3xl leading-none tabular-nums">{value}</span>
      {note && <span className="truncate text-2xs text-faint">{note}</span>}
    </div>
  );
}

/** The headline figures of the runs the filter counts. */
export default function Kpis({ t }: { t: Totals }) {
  const unjudged = t.runs - t.judged;
  const costNote = t.uncosted ? `${t.uncosted} run${t.uncosted === 1 ? '' : 's'} without a known cost left out` : undefined;
  return (
    <div className="grid grid-cols-2 gap-3 md:grid-cols-3 xl:grid-cols-6">
      <Tile label="Runs" value={t.runs} note={unjudged ? `${unjudged} stopped or failed by the platform` : undefined} />
      <Tile label="Passed" value={t.passed} note={`of ${t.judged} judged`} />
      <Tile label="Pass rate" value={pct(t.pass_rate)} />
      <Tile label="Total cost" value={money(t.runs ? t.cost_usd : null)} note={costNote} />
      <Tile label="Cost per passed run" value={money(t.cost_per_pass)} note={t.passed ? 'every run’s cost over the passes' : 'no run passed'} />
      <Tile label="Median run time" value={duration(t.median_seconds)} />
    </div>
  );
}
