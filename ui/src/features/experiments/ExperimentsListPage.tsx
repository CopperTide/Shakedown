// Experiments: every experiment in the project, with sub-experiments under their parent.
import { Plus } from 'lucide-react';
import { useState } from 'react';
import { Link } from 'react-router-dom';
import { experiments as dbExperiments } from '@/api/experiments';
import { listItems } from '@/api/project';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { PageBody, PageHeader } from '@/components/ui/page';
import { EmptyState, ErrorState } from '@/components/ui/state';
import { Toggle } from '@/components/ui/toggle';
import { useQuery } from '@/hooks/useQuery';
import EarlierExperiments from './EarlierExperiments';
import ExperimentRows from './ExperimentRows';
import { type Row, filterExperiments, ordered } from './list';

/** Every file experiment with the fields the list needs, all in the list's one answer. */
async function loadRows(): Promise<Row[]> {
  const { items } = await listItems('experiments');
  return items.map((s) => ({ id: s.id, label: s.label, scenario: s.scenario ?? '', setups: s.setups ?? 0, problems: s.problems, parent: s.parent ?? undefined }));
}

export default function ExperimentsListPage() {
  const rows = useQuery('experiment-files', loadRows);
  const earlier = useQuery('experiments-db', dbExperiments);
  const [query, setQuery] = useState('');
  const [problemsOnly, setProblemsOnly] = useState(false);
  const shown = ordered(filterExperiments(rows.data ?? [], { query, problemsOnly }));
  const newButton = <Button variant="primary" asChild><Link to="/experiments/new"><Plus />New experiment</Link></Button>;
  return (
    <>
      <PageHeader title="Experiments" actions={newButton} meta="Each experiment runs one scenario against the agent setups you compare. Branch one to make a sub-experiment." width="wide" />
      <PageBody width="wide">
        {rows.error && <ErrorState error={String(rows.error)} onRetry={rows.reload} />}
        <Card>
          <div className="flex flex-wrap items-center gap-4 border-b border-line px-[18px] py-3">
            <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search experiments or scenarios" className="max-w-sm" aria-label="Search" />
            <Toggle checked={problemsOnly} onChange={setProblemsOnly} label="With problems only" />
          </div>
          {rows.data && rows.data.length === 0 ? <EmptyState title="No experiments yet." action={newButton} /> : <ExperimentRows rows={shown} onDeleted={rows.reload} />}
        </Card>
        <EarlierExperiments list={earlier.data ?? []} onMoved={() => { rows.reload(); earlier.reload(); }} />
      </PageBody>
    </>
  );
}
