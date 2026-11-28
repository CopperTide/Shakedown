// Every experiment that plays the scenario, each a link: sub-experiments under their parent, and
// whether each has run.
import { CornerDownRight } from 'lucide-react';
import { Link } from 'react-router-dom';
import { Card } from '@/components/ui/card';
import { StatusMark } from '@/components/ui/status';
import { ordered, rowOf } from '@/features/experiments/list';
import { RUN_STATUS_MARK, runStatusTitle } from '@/features/experiments/runStatus';
import { useRunStatuses } from '@/features/experiments/useRunStatuses';
import type { ItemSummary } from '@/model/project';

export default function ScenarioExperiments({ experiments }: { experiments: ItemSummary[] }) {
  const statuses = useRunStatuses(experiments.map((e) => e.id));
  const rows = ordered(experiments.map(rowOf));
  const n = rows.length;
  return (
    <Card title="Experiments" description={n ? `${n} experiment${n === 1 ? '' : 's'}` : undefined}>
      {n === 0
        ? <p className="px-[18px] py-3 text-sm text-faint">No experiments yet.</p>
        : (
          <ul className="divide-y divide-line">
            {rows.map((r) => (
              <li key={r.id}>
                <Link to={`/experiments/file/${r.id}`} className="flex items-center gap-3 px-[18px] py-2.5 text-sm transition-colors hover:bg-hover">
                  <span className="flex min-w-0 flex-1 items-center font-medium" style={{ paddingLeft: r.depth * 20 }}>
                    {r.depth > 0 && <CornerDownRight className="mr-1.5 size-3.5 shrink-0 text-faint" aria-hidden="true" />}
                    <span className="truncate">{r.label}</span>
                  </span>
                  {statuses[r.id] && <StatusMark mark={RUN_STATUS_MARK[statuses[r.id].status]} title={runStatusTitle(statuses[r.id])} />}
                  <span className="w-16 text-right text-xs text-faint tabular-nums">{r.setups} setup{r.setups === 1 ? '' : 's'}</span>
                </Link>
              </li>
            ))}
          </ul>
        )}
    </Card>
  );
}
