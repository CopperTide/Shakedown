// The runs or steps behind one figure of the builder, each opening its run page (a step at its
// place in the run, via ?step=).
import { Link } from 'react-router-dom';
import { type DrillResult, type DrillRun, type DrillStep, exploreDrill, type Spec } from '@/api/explore';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import { ErrorState, LoadingState } from '@/components/ui/state';
import { StepLabel } from '@/components/ui/step-label';
import { useQuery } from '@/hooks/useQuery';
import { fmt, duration } from '@/lib/format';
import { ago, formatTime } from '@/lib/time';
import { path } from '@/lib/url';
import type { DrillAt } from './ResultTable';

const stepHref = (s: DrillStep) => `${path`/runs/${s.run_id}/${s.branch_id}`}?step=${s.seq}`;

function RunRow({ r }: { r: DrillRun }) {
  return (
    <li>
      <Link to={path`/runs/${r.run_id}/${r.branch_id}`} className="flex flex-col gap-0.5 px-4 py-2 hover:bg-hover focus-visible:bg-hover">
        <span className="flex items-baseline gap-2 text-sm">
          <span className="min-w-0 flex-1 truncate">{r.name || r.scenario}</span>
          <span className="text-xs text-muted">{r.status}</span>
        </span>
        <span className="flex flex-wrap gap-x-3 text-xs text-muted">
          <span className="truncate">{r.setup}</span>
          <span className="tabular-nums">{fmt(r.cost_usd, 'usd')}</span>
          <span className="tabular-nums">{duration(r.seconds)}</span>
          <time dateTime={r.at} title={formatTime(r.at)} className="ml-auto text-faint">{ago(r.at)}</time>
        </span>
      </Link>
    </li>
  );
}

function StepRow({ s }: { s: DrillStep }) {
  return (
    <li>
      <Link to={stepHref(s)} className="flex flex-col gap-0.5 px-4 py-2 hover:bg-hover focus-visible:bg-hover">
        <span className="flex flex-wrap items-center gap-2 text-xs">
          <StepLabel seq={s.seq} kind={(s.tool ?? s.kind).replace(/_/g, ' ')}>{s.agent && <span className="text-muted">{s.agent}</span>}</StepLabel>
          {s.outcome && s.outcome !== 'ok' && <span className={s.outcome === 'error' ? 'text-alarm' : 'text-muted'}>{s.outcome}</span>}
          <time dateTime={s.at} title={formatTime(s.at)} className="ml-auto text-2xs text-faint">{ago(s.at)}</time>
        </span>
        <span className="truncate text-xs text-muted">{s.run_name}{s.model ? ` · ${s.model}` : ''}</span>
      </Link>
    </li>
  );
}

function Items({ d }: { d: DrillResult }) {
  const n = d.kind === 'runs' ? d.runs.length : d.steps.length;
  return (
    <>
      <p role="status" className="border-b border-line px-4 py-2 text-xs text-muted">
        {n === 0 ? 'Nothing behind this figure.' : `${n.toLocaleString()}${d.capped ? ' newest' : ''} ${d.kind === 'runs' ? 'run' : 'step'}${n === 1 ? '' : 's'}`}
      </p>
      <ul className="min-h-0 flex-1 divide-y divide-line overflow-y-auto">
        {d.kind === 'runs' ? d.runs.map((r) => <RunRow key={r.branch_id} r={r} />) : d.steps.map((s) => <StepRow key={`${s.branch_id}:${s.seq}`} s={s} />)}
      </ul>
    </>
  );
}

export default function DrillDialog({ spec, at, onClose }: { spec: Spec; at: DrillAt | null; onClose: () => void }) {
  const input = at && { spec, cell: at.cell, measure: at.measure };
  const q = useQuery(input ? `explore:drill:${JSON.stringify(input)}` : null, () => exploreDrill(input!));
  return (
    <Dialog open={at != null} onOpenChange={(o) => { if (!o) onClose(); }}>
      {at && (
        <DialogContent title={at.title} description="Open one to see it in its run." className="w-[min(640px,calc(100vw-24px))]">
          {q.data ? <Items d={q.data} /> : q.error ? <ErrorState error={q.error} onRetry={q.retry} className="m-4" /> : <LoadingState label="Finding them…" rows={3} className="m-4" />}
        </DialogContent>
      )}
    </Dialog>
  );
}
