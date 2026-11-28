import { Hint } from '@/components/ui/hint';
import type { WeekStats } from './home';

/** A figure: its label (what it counts on hovering it), the value, and an optional short data line. */
function Figure({ label, hint, value, sub }: { label: string; hint: string; value: string; sub?: string }) {
  return (
    <div className="min-w-0 border-l border-line px-[18px] py-3.5 first:border-l-0">
      <div className="text-xs text-faint"><Hint hint={hint}>{label}</Hint></div>
      <div className="mt-1 font-display text-[30px] leading-none text-fg tabular-nums">{value}</div>
      {sub && <div className="mt-1.5 text-xs text-faint">{sub}</div>}
    </div>
  );
}

/** This week in four figures. */
export default function Stats({ stats }: { stats: WeekStats }) {
  const rate = stats.passRate == null ? '–' : `${Math.round(stats.passRate * 100)}%`;
  return (
    <section aria-label="This week" className="grid grid-cols-2 rounded-lg border border-line bg-surface shadow-panel md:grid-cols-4">
      <Figure label="Runs this week" hint="Runs started in the last 7 days" value={String(stats.runs)} />
      <Figure label="Pass rate" hint="Passed of finished, this week" value={rate} sub={`${stats.passed} of ${stats.passed + stats.failed}`} />
      <Figure label="Failed" hint="Checks failed, over budget, an error, interrupted or stopped. Open them under Needs attention." value={String(stats.failed)} />
      <Figure label="Running now" hint="Across every experiment" value={String(stats.running)} />
    </section>
  );
}
