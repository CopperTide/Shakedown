import { useMemo, useState } from 'react';
import { useParams } from 'react-router-dom';
import { StatusChip } from '@/components/marks';
import { PageBody, PageGate, PageHeader } from '@/components/ui/page';
import { Panel, PanelHeader } from '@/components/ui/panel';
import { Split } from '@/components/ui/split';
import ActionTimeline from '@/features/run/ActionTimeline';
import Inspector from '@/features/run/Inspector';
import RunMenu from '@/features/run/RunMenu';
import { RunErrorBanner, StepSidePanel, useExpanded } from '@/features/run/RunShell';
import { taskOf } from '@/features/run/runStats';
import { useBranch } from '@/features/run/useBranch';
import { useStepKeys } from '@/features/run/useStepKeys';
import { useStepParam } from '@/features/run/useStepParam';
import { type Timeline, useTimeline } from '@/features/run/useTimeline';
import { useTaskNames } from '@/hooks/useTasks';
import { actionsOf, worldInfo } from '@/model/actions';
import type { BranchDetail } from '@/model/types';
import { processView } from './process';
import ProcessDiagram from './ProcessDiagram';
import TrendsPanel from './TrendsPanel';

/** The selected step, in the address (`?step=`) the run page and the world view share. */
function useSelection(order: number[]) {
  const [selected, setSelected] = useStepParam();
  useStepKeys(order, setSelected);
  return [selected, setSelected] as const;
}

function Main({ d, timeline: { lanes, span }, selected, onSelect, models, setModels }: {
  d: BranchDetail; timeline: Timeline; selected: number | null; onSelect: (s: number) => void; models: boolean; setModels: (m: boolean) => void;
}) {
  const actions = useMemo(() => actionsOf(d.events, d.start_seq), [d]);
  const view = useMemo(() => processView(d.events, actions, d.team), [d, actions]);
  const running = d.status === 'running';
  return (
    <div className="flex min-w-0 flex-col gap-3">
      <Panel><PanelHeader title="Process" meta="Agents as units, systems as equipment; heavy lines write, dashed lines read" /><ProcessDiagram view={view} /></Panel>
      <Panel><PanelHeader title={`Actions${running ? ' · live' : ''}`} />
        <ActionTimeline lanes={lanes} team={d.team} span={span} selected={selected} onSelect={onSelect} models={models} onModels={setModels} />
      </Panel>
      <Panel><PanelHeader title="Trends" /><TrendsPanel events={d.events} actions={actions} /></Panel>
    </div>
  );
}

/** The run as a process (L2), its action timeline and trends, and one step's trace (L4). Live while it runs. */
export default function ProcessPage() {
  const { runId = '', branchId = '' } = useParams();
  const { detail: d, error, reload } = useBranch(branchId);
  const [models, setModels] = useState(true);
  const nameOf = useTaskNames();
  const timeline = useTimeline(d, models);
  const [selected, setSelected] = useSelection(timeline.order);
  const [expanded, setExpanded] = useExpanded();
  if (!d) return <PageGate title="Process" label="Loading the run…" rows={5} error={error} onRetry={reload} />;
  const task = taskOf(d.events);
  const info = worldInfo(d.events);
  return (
    <>
      <PageHeader title={`Process · ${nameOf(task) || 'Run'}`} status={<StatusChip status={d.status} />}
        meta={!info.known && 'Recorded before Shakedown kept each tool\'s system and effect: those are shown as unknown.'}
        actions={<RunMenu runId={runId} d={d} page="process" step={selected} />} />
      <PageBody>
        <RunErrorBanner error={error} onRetry={reload} detail={d} />
        <Split storageKey="shakedown.process.side" expanded={expanded} main={<Main d={d} timeline={timeline} selected={selected} onSelect={setSelected} models={models} setModels={setModels} />}
          side={<StepSidePanel expanded={expanded} onExpand={setExpanded}><Inspector branchId={d.id} event={d.events.find((e) => e.seq === selected) ?? null} team={d.team} events={d.events} /></StepSidePanel>} />
      </PageBody>
    </>
  );
}
