// A file experiment's deployments: when each started, how far it got, and a link to its results.
import { Link } from 'react-router-dom';
import { experimentRuns } from '@/api/project';
import { Card } from '@/components/ui/card';
import { EmptyState, ErrorState } from '@/components/ui/state';
import { useQuery } from '@/hooks/useQuery';

export default function Deployments({ id, version }: { id: string; version: number }) {
  const q = useQuery(`experiment-runs:${id}:${version}`, () => experimentRuns(id));
  if (q.error) return <ErrorState error={q.error} onRetry={q.reload} />;
  const list = q.data?.deployments ?? [];
  if (q.data && list.length === 0) return <EmptyState title="No runs yet" hint="Deploy runs this experiment." />;
  return (
    <div className="grid gap-3">
      {list.map((d) => (
        <Card key={d.id} title={new Date(d.created_at).toLocaleString()} description={d.status}
          actions={<Link to={`/experiments/${d.id}`} className="text-sm text-accent">Open results</Link>}>
          <ul className="divide-y divide-line">
            {d.setups.map((s) => (
              <li key={s.id} className="flex items-center gap-3 px-[18px] py-2.5 text-sm">
                <span className="flex-1 font-medium">{s.name}</span>
                <span className="tabular-nums text-faint">{s.summary.passed} passed of {s.summary.ended} finished</span>
              </li>
            ))}
          </ul>
        </Card>
      ))}
    </div>
  );
}
