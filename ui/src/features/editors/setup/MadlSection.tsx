// A MADL team: its checks, and its agents and connections drawn from the file.
import { madlOf } from '@/api/project';
import { Card } from '@/components/ui/card';
import { ErrorState } from '@/components/ui/state';
import { useQuery } from '@/hooks/useQuery';
import { SectionHead } from '../fields';
import TeamGraph from './TeamGraph';

export default function MadlSection({ id }: { id: string }) {
  const q = useQuery(`madl:${id}`, () => madlOf(id));
  return (
    <div className="grid gap-4">
      <SectionHead title="Team graph" about="Read from the team's MADL file: every agent and connection, and the MADL checks." />
      {q.error && <ErrorState error={q.error} onRetry={q.reload} />}
      {q.data && (
        <>
          <Card title={q.data.valid ? 'Valid MADL' : 'Problems in the MADL file'}>
            {q.data.diagnostics.length === 0
              ? <p className="px-[18px] py-3 text-sm text-ok">Every check passed.</p>
              : <ul className="divide-y divide-line">{q.data.diagnostics.map((d, i) => (
                  <li key={i} className="grid gap-0.5 px-[18px] py-2.5 text-sm">
                    <span className={d.severity === 'error' ? 'text-alarm' : 'text-warn'}>{d.message}</span>
                    <span className="font-mono text-2xs text-faint">{d.code} · {d.pointer}</span>
                  </li>))}</ul>}
          </Card>
          <Card title="Agents and connections"><TeamGraph graph={q.data.graph} /></Card>
        </>
      )}
    </div>
  );
}
