// Get started: from nothing to a first scenario, one step at a time. Each step ticks itself off
// from what exists, and the page moves on to the next open step by itself; nothing is filled in for you. There is no skipping: the rest of Shakedown opens
// once the steps are done (see gate.tsx).
import { Check } from 'lucide-react';
import { useEffect, useState } from 'react';
import { AccountMenu, useMe } from '@/app/AccountMenu';
import { Mark } from '@/components/brand/Mark';
import { useProject } from '@/hooks/useProject';
import { useProviderList } from '@/hooks/useProviders';
import { cn } from '@/lib/cn';
import StepItems from './StepItems';
import StepModel from './StepModel';
import StepProject from './StepProject';
import StepReady from './StepReady';
import { type StepState, firstOpen, stageOf, stepStates } from './steps';

function Rail({ steps, active, onSelect }: { steps: StepState[]; active: string; onSelect: (id: StepState['id']) => void }) {
  return (
    <nav aria-label="Steps" className="flex flex-col gap-1">
      {steps.map((s, i) => (
        <button key={s.id} type="button" onClick={() => onSelect(s.id)} aria-current={s.id === active ? 'step' : undefined}
          className={cn('flex items-center gap-3 rounded-lg px-3 py-2 text-left text-sm', s.id === active ? 'bg-surface shadow-panel' : 'hover:bg-hover')}>
          <span className={cn('grid size-6 place-items-center rounded-full border text-2xs', s.done ? 'border-fg bg-fg text-bg' : s.id === active ? 'border-copper' : 'border-line-strong')}>
            {s.done ? <Check className="size-3.5" /> : i + 1}
          </span>{s.title}
        </button>
      ))}
    </nav>
  );
}

function Body({ id, steps, ctx }: { id: StepState['id']; steps: StepState[]; ctx: { stage: ReturnType<typeof stageOf>; name?: string; folder: boolean; counts: Record<string, number>; providersOk: boolean; reload: () => void } }) {
  switch (id) {
    case 'model': return <StepModel ready={ctx.providersOk} />;
    case 'world': return <StepItems kind="worlds" count={ctx.counts.worlds ?? 0} about="Your production systems before any agent: connectors, software flows and the data every run starts from." prompt="Draft a world for my systems. Here is what we run: " />;
    case 'agents': return <StepItems kind="setups" count={ctx.counts.setups ?? 0} about="The agent team you want to test: built-in agents, your own code, an endpoint, Claude Code or Codex. A MADL file gets the full graph." prompt="Set up an agent team that " />;
    case 'scenario': return <StepItems kind="scenarios" count={ctx.counts.scenarios ?? 0} about="An incident to replay. Paste a postmortem or a ticket and the assistant drafts the inputs, events and checks." prompt="Draft a scenario from this incident: " />;
    case 'ready': return <StepReady steps={steps} />;
    default: return <StepProject stage={ctx.stage} name={ctx.name} folder={ctx.folder} onCreated={ctx.reload} />;
  }
}

/**
 * The step shown: the one picked, else the first open one. A step picked while open hands over to
 * the next open one as soon as it is done; one picked after it was done stays until another is picked.
 */
function useActiveStep(steps: StepState[]) {
  const [picked, setPicked] = useState<{ id: StepState['id']; wasDone: boolean } | null>(null);
  const done = (id?: StepState['id']) => !!steps.find((s) => s.id === id)?.done;
  const handOver = !!picked && !picked.wasDone && done(picked.id);
  useEffect(() => { if (handOver) setPicked(null); }, [handOver]);
  const pick = (id: StepState['id']) => setPicked({ id, wasDone: done(id) });
  return [picked?.id ?? firstOpen(steps), pick] as const;
}

/** What the steps read from the project answer and the providers. */
function contextOf(project: ReturnType<typeof useProject>, providersOk: boolean) {
  const open = project.data?.open ? project.data.status : null;
  const closed = project.data && !project.data.open ? project.data : undefined;
  return { stage: stageOf(!!open, closed?.message), name: open?.name, folder: !!closed?.folder, counts: open?.counts ?? {}, providersOk, reload: project.reload };
}

export default function GetStartedPage() {
  const project = useProject();
  const me = useMe();
  const providers = useProviderList();
  const ctx = contextOf(project, providers.list.some((p) => p.available));
  const steps = stepStates({ project: ctx.stage, counts: ctx.counts, providersOk: ctx.providersOk });
  const [active, pick] = useActiveStep(steps);
  const current = steps.find((s) => s.id === active);
  const loading = !project.data && !project.error;
  return (
    <div className="min-h-full bg-bg">
      <header className="flex items-center gap-2 border-b border-line px-7 py-3"><Mark /><span className="font-display text-lg">Shakedown</span><span className="flex-1" /><AccountMenu me={me} /></header>
      <div className="mx-auto grid max-w-5xl gap-10 px-7 py-10 md:grid-cols-[240px_minmax(0,1fr)]">
        <Rail steps={steps} active={active} onSelect={pick} />
        {!loading && <main><h1 className="mb-6 font-display text-[34px] leading-tight">{current?.title}</h1><Body id={active} steps={steps} ctx={ctx} /></main>}
      </div>
    </div>
  );
}
