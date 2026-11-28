// On a run's page, its experiment's other runs and this run's reruns, folded to one line: go to
// another run without the experiment page, or tick several and compare them.
import { ChevronDown, ChevronLeft, ChevronRight, ChevronUp } from 'lucide-react';
import { type ReactNode, useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { StatusChip } from '@/components/marks';
import { Button } from '@/components/ui/button';
import { Panel } from '@/components/ui/panel';
import { compareHref } from '@/features/compare/many';
import RunStrip from '@/features/experiments/RunStrip';
import { branchTree } from '@/features/runs/tree';
import { useQuery } from '@/hooks/useQuery';
import { usePref } from '@/hooks/usePref';
import { cn } from '@/lib/cn';
import type { Experiment } from '@/model/experiment';
import type { Branch, BranchDetail } from '@/model/types';
import { experimentOfRun, switchOf } from './experimentOf';
import { taskOf } from './runStats';

type Picks = { picked: ReadonlySet<string>; toggle: (branchId: string) => void } | null;

/** The branches ticked for comparing, while picking; picking starts with the branch on screen ticked. */
function usePicks(current: string) {
  const [picking, setPicking] = useState(false);
  const [picked, setPicked] = useState<ReadonlySet<string>>(new Set());
  const toggle = (id: string) => setPicked((p) => { const next = new Set(p); if (next.has(id)) next.delete(id); else next.add(id); return next; });
  const start = () => { setPicking(true); setPicked(new Set([current])); };
  const stop = () => { setPicking(false); setPicked(new Set()); };
  return { picking, start, picked, toggle, stop };
}

/** ‹ Setup A · run 2 of 3 ›: the experiment's started runs, one after another across its setups. */
function Hop({ e, runId }: { e: Experiment; runId: string }) {
  const { started, at } = switchOf(e, runId);
  const prev = at > 0 ? started[at - 1] : null;
  const next = at >= 0 && at < started.length - 1 ? started[at + 1] : null;
  const arrow = (to: typeof prev, label: string, icon: ReactNode) => (to?.href
    ? <Button asChild variant="ghost" size="sm" className="size-7 px-0"><Link to={to.href} aria-label={label} title={`${label}: ${to.setup}, run ${to.n}`}>{icon}</Link></Button>
    : <Button variant="ghost" size="sm" className="size-7 px-0" aria-label={label} disabled>{icon}</Button>);
  return (
    <span className="flex items-center gap-0.5 text-xs text-muted">
      {arrow(prev, 'Previous run', <ChevronLeft />)}
      <span className="tabular-nums">{at >= 0 ? `${started[at].setup} · run ${started[at].n} of ${started[at].of}` : `${started.length} runs`}</span>
      {arrow(next, 'Next run', <ChevronRight />)}
    </span>
  );
}

function Setups({ e, runId, picks }: { e: Experiment; runId: string; picks: Picks }) {
  const { columns } = switchOf(e, runId);
  const pick = { current: runId, picked: picks?.picked, onPick: picks ? (c: { branchId: string | null }) => c.branchId && picks.toggle(c.branchId) : undefined };
  return columns.map((c) => (
    <div key={c.id} className="flex items-center gap-3 px-3 py-1.5">
      <span className="w-44 shrink-0 truncate text-xs font-medium" title={c.team}>{c.name}</span>
      <RunStrip runs={c.runs} pick={pick} />
    </div>
  ));
}

/** This run's original and reruns, each a link (a tick box while picking). */
function Reruns({ runId, branchId, siblings, picks }: { runId: string; branchId: string; siblings: Branch[]; picks: Picks }) {
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 px-3 py-1.5">
      <span className="w-44 shrink-0 text-xs font-medium">Reruns</span>
      {branchTree(siblings).map(({ branch: b, depth }) => {
        const name = depth ? `↳ ${b.fork_note ?? 'Rerun'}` : 'Original';
        const ticked = !!picks?.picked.has(b.id);
        const look = cn('flex max-w-56 items-center gap-1.5 rounded-md border px-2 py-0.5 text-xs', b.id === branchId ? 'border-fg' : 'border-line', ticked && 'border-accent bg-accent-soft');
        return picks
          ? <button key={b.id} type="button" aria-pressed={ticked} onClick={() => picks.toggle(b.id)} className={look}><span className="truncate">{name}</span><StatusChip status={b.status} /></button>
          : <Link key={b.id} to={`/runs/${runId}/${b.id}`} className={look}><span className="truncate">{name}</span><StatusChip status={b.status} /></Link>;
      })}
    </div>
  );
}

function PickBar({ p }: { p: ReturnType<typeof usePicks> }) {
  const nav = useNavigate();
  if (!p.picking) return <Button variant="ghost" size="sm" onClick={p.start}>Pick runs to compare</Button>;
  return (
    <span className="flex items-center gap-2 text-xs text-muted">
      {p.picked.size} ticked
      <Button size="sm" variant="primary" disabled={p.picked.size < 2} onClick={() => nav(compareHref([...p.picked]))}>Compare {p.picked.size > 1 ? p.picked.size : ''}</Button>
      <Button size="sm" variant="ghost" onClick={p.stop}>Cancel</Button>
    </span>
  );
}

/** The folded line: open or fold, the experiment and the run's place in it, its results, and picking. */
function Head({ e, runId, open, setOpen, p }: { e: Experiment | null; runId: string; open: boolean; setOpen: (o: boolean) => void; p: ReturnType<typeof usePicks> }) {
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 px-2 py-1">
      <Button variant="ghost" size="sm" aria-expanded={open} onClick={() => setOpen(!open)}>
        {open ? <ChevronUp /> : <ChevronDown />}{e ? <>Experiment · <span className="font-semibold text-fg">{e.name}</span></> : 'Reruns'}
      </Button>
      {e && <><Hop e={e} runId={runId} /><Link to={`/experiments/${e.id}`} className="text-xs text-accent hover:underline">Results</Link></>}
      <span className="ml-auto">{open && <PickBar p={p} />}</span>
    </div>
  );
}

/** The switcher; nothing for a run outside an experiment with no reruns. */
export default function RunSwitcher({ runId, detail, siblings }: { runId: string; detail: BranchDetail; siblings: Branch[] }) {
  const [open, setOpen] = usePref('shakedown.run.switcher', false);
  const p = usePicks(detail.id);
  const e = useQuery(`run-experiment:${runId}`, () => experimentOfRun(runId, taskOf(detail.events) || null, detail.created_at)).data ?? null;
  const reruns = siblings.length > 1;
  if (!e && !reruns) return null;
  const picks: Picks = p.picking ? { picked: p.picked, toggle: p.toggle } : null;
  return (
    <Panel className="mb-3">
      <Head e={e} runId={runId} open={open} setOpen={setOpen} p={p} />
      {open && (
        <div className="border-t border-line py-1">
          {e && <Setups e={e} runId={runId} picks={picks} />}
          {reruns && <Reruns runId={runId} branchId={detail.id} siblings={siblings} picks={picks} />}
        </div>
      )}
    </Panel>
  );
}
