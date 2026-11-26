import { useMemo } from 'react';
import type { Summary } from '@/api/analytics';
import CountBars from './CountBars';
import { durationLabel, usageBuckets } from './moreChartsData';
import { Card } from './parts';
import UsageChart from './UsageChart';

/** Slice 2: totals, failure counts, and duration spread under the overview's existing filters. */
export default function MoreCharts({ summary: s }: { summary: Summary }) {
  const usage = useMemo(() => usageBuckets(s.series), [s.series]);
  const duration = s.duration.map((b) => ({ name: durationLabel(b), runs: b.runs }));
  const judged = duration.reduce((n, r) => n + r.runs, 0);
  return <>
    <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
      <UsageChart rows={usage} metric="cost_usd" bucket={s.bucket} />
      <UsageChart rows={usage} metric="tokens" bucket={s.bucket} />
    </div>
    <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
      <Card title="Failing checks" hint="Affected runs per named check in the final score. A run can fail several checks; counts do not add up to total runs.">
        <CountBars rows={s.failures.checks} label="Runs with failing checks" empty="No named check failures recorded for these runs." />
      </Card>
      <Card title="Refused tools / gates" hint="Affected runs per tool with a recorded refusal. This identifies refused tools, not individual gate IDs; repeats within one run count once, even if that run eventually passes.">
        <CountBars rows={s.failures.gates} label="Runs with refused tools or gates" empty="No tool or gate refusals recorded for these runs." />
      </Card>
      <Card title="Why runs did not pass" hint="Recorded terminal outcomes, not inferred root causes. Stopped, interrupted and platform failures remain separate from judged failures.">
        <CountBars rows={s.failures.reasons} label="Non-passing run outcomes" empty="No non-passing outcomes in these runs." />
      </Card>
      <Card title="Run-time distribution" hint={`${judged.toLocaleString()} judged runs. Wall-clock duration; stopped, interrupted and platform failures excluded. Each bar shows the count in its labeled range, not density.`}>
        <CountBars rows={duration} label="Run-time distribution counts" empty="No judged run durations available." ordered />
      </Card>
    </div>
  </>;
}
