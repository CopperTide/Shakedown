// Experiments: every experiment in the project, with sub-experiments under their parent, and
// whether each has run (filter and sort by it).
import { Plus } from 'lucide-react';
import { useState } from 'react';
import { Link } from 'react-router-dom';
import { experiments as dbExperiments } from '@/api/experiments';
import { listItems } from '@/api/project';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Input, NativeSelect } from '@/components/ui/input';
import { PageBody, PageHeader } from '@/components/ui/page';
import { EmptyState, ErrorState } from '@/components/ui/state';
import { Toggle } from '@/components/ui/toggle';
import { useQuery } from '@/hooks/useQuery';
import EarlierExperiments from './EarlierExperiments';
import ExperimentRows from './ExperimentRows';
import { type Row, type Sort, arrange, filterExperiments, rowOf } from './list';
import { RUN_STATUSES, RUN_STATUS_MARK, type RunStatus } from './runStatus';
import { useRunStatuses } from './useRunStatuses';

/** Every file experiment with the fields the list needs, all in the list's one answer. */
async function loadRows(): Promise<Row[]> {
  return (await listItems('experiments')).items.map(rowOf);
}

const SORTS: { id: Sort; label: string }[] = [{ id: 'tree', label: 'Grouped' }, { id: 'status', label: 'Has results first' }, { id: 'recent', label: 'Last deployed' }];

/** Which experiments to show by status, and their order. */
function StatusControls({ status, setStatus, sort, setSort }: { status: RunStatus | 'all'; setStatus: (s: RunStatus | 'all') => void; sort: Sort; setSort: (s: Sort) => void }) {
  return (
    <>
      <NativeSelect aria-label="Status" value={status} onChange={(e) => setStatus(e.target.value as RunStatus | 'all')} className="w-auto">
        <option value="all">Any status</option>
        {RUN_STATUSES.map((s) => <option key={s} value={s}>{RUN_STATUS_MARK[s].word}</option>)}
      </NativeSelect>
      <NativeSelect aria-label="Sort" value={sort} onChange={(e) => setSort(e.target.value as Sort)} className="w-auto">
        {SORTS.map((s) => <option key={s.id} value={s.id}>{s.label}</option>)}
      </NativeSelect>
    </>
  );
}

/** The file experiments, each with whether it ran, filtered and sorted as asked. */
function useShown(all: Row[], query: string, problemsOnly: boolean, status: RunStatus | 'all', sort: Sort) {
  const statuses = useRunStatuses(all.map((r) => r.id));
  const rows = all.map((r) => ({ ...r, run: statuses[r.id] }));
  return arrange(filterExperiments(rows, { query, problemsOnly, status }), sort);
}

export default function ExperimentsListPage() {
  const rows = useQuery('experiment-files', loadRows);
  const earlier = useQuery('experiments-db', dbExperiments);
  const [query, setQuery] = useState('');
  const [problemsOnly, setProblemsOnly] = useState(false);
  const [status, setStatus] = useState<RunStatus | 'all'>('all');
  const [sort, setSort] = useState<Sort>('tree');
  const shown = useShown(rows.data ?? [], query, problemsOnly, status, sort);
  const newButton = <Button variant="primary" asChild><Link to="/experiments/new"><Plus />New experiment</Link></Button>;
  return (
    <>
      <PageHeader title="Experiments" actions={newButton} meta="Each experiment runs one scenario against the agent setups you compare. Branch one to make a sub-experiment." width="wide" />
      <PageBody width="wide">
        {rows.error && <ErrorState error={String(rows.error)} onRetry={rows.reload} />}
        <Card>
          <div className="flex flex-wrap items-center gap-4 border-b border-line px-[18px] py-3">
            <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search experiments or scenarios" className="max-w-sm" aria-label="Search" />
            <StatusControls status={status} setStatus={setStatus} sort={sort} setSort={setSort} />
            <Toggle checked={problemsOnly} onChange={setProblemsOnly} label="With problems only" />
          </div>
          {rows.data && rows.data.length === 0 ? <EmptyState title="No experiments yet." action={newButton} /> : <ExperimentRows rows={shown} onDeleted={rows.reload} />}
        </Card>
        <EarlierExperiments list={earlier.data ?? []} onMoved={() => { rows.reload(); earlier.reload(); }} />
      </PageBody>
    </>
  );
}
