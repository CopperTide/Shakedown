import type { WeekStats } from './home';

function Figure({ label, value, sub }: { label: string; value: string; sub: string }) {
  return (
    <div className="min-w-0 border-l border-line px-[18px] py-3.5 first:border-l-0">
      <div className="text-xs text-faint">{label}</div>
      <div className="mt-1 font-display text-[30px] leading-none text-fg tabular-nums">{value}</div>
      <div className="mt-1.5 text-xs text-faint">{sub}</div>
    </div>
  );
}

/** This week in four figures. */
export default function Stats({ stats }: { stats: WeekStats }) {
  const rate = stats.passRate == null ? '–' : `${Math.round(stats.passRate * 100)}%`;
  return (
    <section aria-label="This week" className="grid grid-cols-2 rounded-lg border border-line bg-surface shadow-panel md:grid-cols-4">
      <Figure label="Runs this week" value={String(stats.runs)} sub={`${stats.running} running now`} />
      <Figure label="Pass rate" value={rate} sub={`${stats.passed} passed of ${stats.passed + stats.failed} finished`} />
      <Figure label="Failed" value={String(stats.failed)} sub="Open them below" />
      <Figure label="Running now" value={String(stats.running)} sub="Across every experiment" />
    </section>
  );
}
