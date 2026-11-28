// A file experiment: where it sits (world, scenario), any problems in its file, its runs (after it
// is deployed) with a link to each one's results, its configuration folded under them, and Delete.
import { ChevronLeft } from 'lucide-react';
import { useState } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import { ApiError } from '@/api/client';
import { getItem } from '@/api/project';
import { Button } from '@/components/ui/button';
import { PageBody, PageHeader } from '@/components/ui/page';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/state';
import { useQuery } from '@/hooks/useQuery';
import type { ExperimentBody, ItemKind } from '@/model/project';
import DeleteItemDialog from '@/features/editors/DeleteItemDialog';
import DeployDialog from './DeployDialog';
import Deployments from './Deployments';
import ExperimentPath from './ExperimentPath';
import FileConfig, { FileProblems } from './FileConfig';

/** The experiment and the labels of everything it names; null when there is no such experiment. */
async function load(id: string) {
  const item = await getItem<ExperimentBody>('experiments', id).catch((e: unknown) => {
    if (e instanceof ApiError && e.status === 404) return null;
    throw e;
  });
  if (!item) return null;
  const named: [ItemKind, string][] = [['scenarios', item.body.scenario], ['worlds', item.body.world],
    ...item.body.setups.map((s): [ItemKind, string] => ['setups', s]),
    ...(item.body.parent ? [['experiments', item.body.parent] as [ItemKind, string]] : [])];
  const labels: Record<string, string> = {};
  await Promise.all(named.map(async ([kind, name]) => {
    labels[name] = await getItem(kind, name).then((i) => i.label, () => name);
  }));
  return { item, labels };
}

function NotFound({ id }: { id: string }) {
  return (
    <PageBody width="wide">
      <EmptyState title={`No experiment named ${id}`} action={<Button asChild><Link to="/experiments"><ChevronLeft />Experiments</Link></Button>} />
    </PageBody>
  );
}

export default function FileExperimentPage() {
  const { id = '' } = useParams();
  const q = useQuery(`experiment-file:${id}`, () => load(id));
  const [version, setVersion] = useState(0);
  const navigate = useNavigate();
  if (q.error) return <PageBody><ErrorState error={q.error} onRetry={q.reload} /></PageBody>;
  if (q.data === null && !q.loading) return <NotFound id={id} />;
  if (!q.data) return <PageBody><LoadingState label="Loading the experiment" /></PageBody>;
  const { item, labels } = q.data;
  const actions = (
    <>
      <DeleteItemDialog kind="experiments" id={id} label={item.label} onDeleted={() => navigate('/experiments')} />
      <DeployDialog id={id} body={item.body} onDeployed={() => setVersion((v) => v + 1)} />
    </>
  );
  return (
    <>
      <PageHeader title={item.label} actions={actions} width="wide" meta={<ExperimentPath body={item.body} labels={labels} />} />
      <PageBody width="wide">
        <FileProblems problems={item.problems} />
        <h2 className="text-sm font-semibold">Runs</h2>
        <Deployments id={id} version={version} />
        <FileConfig id={id} body={item.body} revision={item.revision} labels={labels} onSaved={q.reload} />
      </PageBody>
    </>
  );
}
