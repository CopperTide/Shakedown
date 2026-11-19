import { useParams } from 'react-router-dom';
import { StatusChip } from '@/components/marks';
import { PageBody, PageGate, PageHeader } from '@/components/ui/page';
import { Panel } from '@/components/ui/panel';
import { EmptyState } from '@/components/ui/state';
import RunMenu from '@/features/run/RunMenu';
import { taskOf } from '@/features/run/runStats';
import { useBranch } from '@/features/run/useBranch';
import { recordedLine } from '@/features/scenario/scenario';
import type { BranchView } from '@/model/twin';
import type { BranchDetail, RunStartPayload } from '@/model/types';
import { drivenLine } from './page';
import TwinView, { useWorldView } from './TwinView';
import { useViews } from './useWorld';

/** The twin's title and the run's status; the task, the disruption and the rest are its tooltip, the run's other views under ⋯. */
function Header({ runId, d, title, step }: { runId: string; d: BranchDetail; title: string; step: number | null }) {
  const driven = drivenLine(d.events);
  const scenario = recordedLine((d.events.find((e) => e.kind === 'run_start')?.payload as RunStartPayload | undefined)?.scenario);
  const tip = [`Digital twin · task ${taskOf(d.events)}`, driven, d.fork_note, scenario && `▼ ${scenario}`].filter(Boolean).join('\n');
  return <PageHeader title={<span title={tip}>{title}</span>} status={<StatusChip status={d.status} />} actions={<RunMenu runId={runId} d={d} page="world" step={step} />} />;
}

function Body({ runId, d, views }: { runId: string; d: BranchDetail; views: BranchView[] }) {
  const w = useWorldView(d, views);
  if (!w.s.view) return null;
  return (
    <>
      <Header runId={runId} d={d} title={w.s.view.title} step={w.s.current} />
      <PageBody className="gap-2.5"><TwinView w={w} views={views} /></PageBody>
    </>
  );
}

/** The digital twin of the run's world at any step: an HMI overview and a scrubber. */
export default function World() {
  const { runId = '', branchId = '' } = useParams();
  const { detail, error, reload } = useBranch(branchId, runId);
  const { views, error: viewError } = useViews(branchId, detail);
  if (!detail) return <PageGate title="World" label="Loading the world…" rows={6} error={error} onRetry={reload} />;
  if (!views.length) {
    return (
      <><PageHeader title="World" status={<StatusChip status={detail.status} />} actions={<RunMenu runId={runId} d={detail} page="world" step={null} />} />
        <PageBody><Panel>
          <EmptyState title="This run has no view of its world." hint={viewError ? `Couldn't load the views: ${viewError}` : undefined} />
        </Panel></PageBody></>
    );
  }
  return <Body runId={runId} d={detail} views={views} />;
}
