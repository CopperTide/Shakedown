import { ChevronDown, ChevronUp } from 'lucide-react';
import { useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { branch as fetchBranch, views as fetchViews } from '@/api/branches';
import { compare } from '@/api/compare';
import { track } from '@/lib/productAnalytics';
import { ApproximateMark, StatusChip } from '@/components/marks';
import { Button } from '@/components/ui/button';
import { CodeBlock } from '@/components/ui/code-block';
import { PageBody, PageHeader } from '@/components/ui/page';
import { Hint } from '@/components/ui/hint';
import { Panel, PanelHeader } from '@/components/ui/panel';
import { ErrorState, LoadingState, Notice } from '@/components/ui/state';
import { StepLabel } from '@/components/ui/step-label';
import { stepHref } from '@/features/review/present';
import WhatMatters from '@/features/review/WhatMatters';
import { formatCost } from '@/features/run/timeline';
import { normalizeViews } from '@/features/world/twin';
import { useQuery } from '@/hooks/useQuery';
import { pretty } from '@/lib/format';
import { asRecord } from '@/lib/record';
import { agentModelLine, costIsUnknown, teamProviders } from '@/model/providers';
import { APPROXIMATE_NOTE, isApproximate } from '@/model/restore';
import type { BranchView, TwinView } from '@/model/twin';
import type { BranchDetail, CompareResponse, CompareSide, Event, Team } from '@/model/types';
import { divergenceStrip } from './divergence';
import OutcomePanel, { DivergenceStrip, STRIP_HINT } from './OutcomePanel';

function StepView({ label, id, run, ev }: { label: string; id: string; run?: string; ev: Event | undefined }) {
  const p = asRecord(ev?.payload);
  return (
    <Panel>
      <PanelHeader title={`${label} at this step`} meta={<span className="font-mono">{id.slice(0, 8)}</span>}>
        {ev && run && <Link className="text-xs text-accent hover:underline" to={`/runs/${run}/${id}?step=${ev.seq}`}>Open step</Link>}
      </PanelHeader>
      {!ev ? <p className="p-3 text-sm text-muted">Ended before this step</p> : (
        <div className="flex flex-col gap-2 p-3">
          <div className="flex flex-wrap items-center gap-1.5 text-xs">
            <StepLabel seq={ev.seq} kind={ev.kind}><span className="text-muted">{ev.agent ?? 'run'}</span></StepLabel>
            {typeof p.name === 'string' && <span className="font-mono">{p.name}</span>}
          </div>
          <CodeBlock className="max-h-64 whitespace-pre-wrap break-all">{pretty(ev.kind === 'tool_call' ? p.args : p)}</CodeBlock>
        </div>
      )}
    </Panel>
  );
}

/** One branch's outcome, numbers and agents (each with its model and where it runs). */
export function Side({ id, s, team, label = '' }: { id: string; s: CompareSide; team?: Team; label?: string }) {
  const unknown = teamProviders(team).some(costIsUnknown);
  const row = (k: string, v: React.ReactNode) => <><dt className="text-2xs font-medium text-muted">{k}</dt><dd>{v}</dd></>;
  return (
    <Panel>
      <PanelHeader title={label || 'Branch'} meta={<span className="font-mono">{id.slice(0, 8)}</span>}><StatusChip status={s.status} /><ApproximateMark of={s} /></PanelHeader>
      <dl className="grid grid-cols-[max-content_1fr] items-baseline gap-x-4 gap-y-1.5 p-3 text-sm">
        {row('Seconds', <span className="font-mono">{s.seconds != null ? s.seconds.toFixed(1) : '–'}</span>)}
        {row('Tokens', <span className="font-mono">{s.tokens?.toLocaleString() ?? '–'}</span>)}
        {row('Cost', formatCost(s.cost_usd, s.cost_source, unknown))}
        {team && row('Agents', <ul className="flex flex-col gap-0.5 text-xs">{team.agents.map((a) => <li key={a.id}>{a.name}: <span className="text-2xs text-muted">{agentModelLine(a)}</span></li>)}</ul>)}
      </dl>
    </Panel>
  );
}

interface Loaded {
  data: CompareResponse;
  a: BranchDetail;
  b: BranchDetail;
  /** Each branch's twin at the end, from the views endpoint; undefined when it can't be read (the events' own views are used). */
  va?: TwinView | null;
  vb?: TwinView | null;
}

/** The branch's last view from `GET /api/branches/{id}/views`; undefined when the endpoint fails. */
export const endView = (id: string) => fetchViews(id).then((v) => v, () => undefined);

export const lastOf = (raw: BranchView[] | undefined): TwinView | null | undefined =>
  raw === undefined ? undefined : normalizeViews(raw).at(-1)?.view ?? null;

async function loadCompare(a: string, b: string): Promise<Loaded> {
  const [data, x, y, rx, ry] = await Promise.all([compare(a, b), fetchBranch(a), fetchBranch(b), endView(a), endView(b)]);
  track('compare opened', { of: 'runs' });
  return { data, a: x, b: y, va: lastOf(rx), vb: lastOf(ry) };
}

function useCompare(a: string, b: string) {
  const { data: got, error, retry } = useQuery(`${a}|${b}`, () => loadCompare(a, b));
  return { got, error, retry };
}

function Divergence({ got, a, b }: { got: Loaded; a: string; b: string }) {
  const seq = got.data.first_divergent_seq;
  return (
    <>
      <Panel className="px-3 py-2 text-sm">
        {seq != null ? <span className="font-semibold"><Hint hint="Everything before it is the same in both branches.">First divergence at step {seq}</Hint></span>
          : <span className="font-semibold">No divergence</span>}
      </Panel>
      {seq != null && (
        <div className="grid gap-3 lg:grid-cols-2">
          <StepView label="A" id={a} run={got.a.run_id} ev={got.a.events.find((e) => e.seq === seq)} />
          <StepView label="B" id={b} run={got.b.run_id} ev={got.b.events.find((e) => e.seq === seq)} />
        </div>
      )}
    </>
  );
}

/** Everything under the outcomes: the divergence strip, each side's figures and the first differing step. */
function Details({ got, a, b }: { got: Loaded; a: string; b: string }) {
  return (
    <>
      <Panel><PanelHeader title={<Hint hint={STRIP_HINT}>Steps</Hint>} /><DivergenceStrip s={divergenceStrip(got.a, got.b, got.data.first_divergent_seq)} /></Panel>
      <div className="grid gap-3 lg:grid-cols-2"><Side label="A" id={a} s={got.data.a} team={got.a.team} /><Side label="B" id={b} s={got.data.b} team={got.b.team} /></div>
      {(isApproximate(got.data.a) || isApproximate(got.data.b)) && <Notice><Hint hint={APPROXIMATE_NOTE}>Approximate restore</Hint></Notice>}
      <Divergence got={got} a={a} b={b} />
    </>
  );
}

/** What matters and the outcomes; the rest behind Details. */
function Comparison({ got, a, b }: { got: Loaded; a: string; b: string }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <WhatMatters target={{ kind: 'compare', a, b }} what="both runs and their comparison"
        hrefOf={(side, seq) => (typeof seq === 'number' ? stepHref(side === 'a' ? got.a.run_id : got.b.run_id, side === 'a' ? a : b, seq) : null)} />
      <OutcomePanel a={{ detail: got.a, side: got.data.a, view: got.va }} b={{ detail: got.b, side: got.data.b, view: got.vb }} />
      <Button variant="ghost" className="self-center text-muted" aria-expanded={open} onClick={() => setOpen(!open)}>{open ? <><ChevronUp />Hide details</> : <><ChevronDown />Details</>}</Button>
      {open && <Details got={got} a={a} b={b} />}
    </>
  );
}

export default function Compare() {
  const { a = '', b = '' } = useParams();
  const { got, error, retry } = useCompare(a, b);
  return (
    <>
      <PageHeader title={<span title={`Branch ${a.slice(0, 8)} (A) against ${b.slice(0, 8)} (B)`}>Compare</span>} width="broad" />
      <PageBody width="broad">
        {error && <ErrorState error={error} onRetry={retry} hint="Both branches must have finished and kept their final saves." />}
        {!got && !error && <LoadingState label="Comparing…" rows={4} />}
        {got && <Comparison got={got} a={a} b={b} />}
      </PageBody>
    </>
  );
}
