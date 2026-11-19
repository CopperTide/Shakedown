import { type ReactElement, useEffect, useState } from 'react';
import { rows } from '@/api/branches';
import { Button } from '@/components/ui/button';
import { CodeBlock } from '@/components/ui/code-block';
import { EmptyState, ErrorState } from '@/components/ui/state';
import { StatusMark, Tag } from '@/components/ui/status';
import { StepLabel } from '@/components/ui/step-label';
import { cn } from '@/lib/cn';
import { errorMessage } from '@/lib/errors';
import { pretty } from '@/lib/format';
import { MARK, alarmMark, mark, passMark } from '@/lib/mark';
import { asRecord } from '@/lib/record';
import { actionWord, classify, systemLine, worldInfo } from '@/model/actions';
import { reasoningWord, wakeWord } from '@/model/lanes';
import { missingPermit, permitOf } from '@/model/permit';
import { costIsUnknown, providerId, providerName, providerOf } from '@/model/providers';
import { notComparedLine, savedLine } from '@/model/restore';
import type { AgentEndPayload, CheckPayload, Event, ModelCallPayload, ReasoningPayload, Team, WakePayload } from '@/model/types';
import { formatCost, rowKeys } from './timeline';

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-1 border-t border-line px-3 py-2">
      <h3 className="text-2xs font-medium text-muted">{title}</h3>
      {children}
    </section>
  );
}

function Json({ value, error = false }: { value: unknown; error?: boolean }) {
  return <CodeBlock className={cn('max-h-72 leading-4 whitespace-pre-wrap break-all', error && 'text-alarm')}>{pretty(value)}</CodeBlock>;
}

function Block({ title, value }: { title: string; value: unknown }) {
  return <Section title={title}><Json value={value} /></Section>;
}

function keysOf(v: unknown): string[] {
  return Array.isArray(v) ? v.filter((k): k is string => typeof k === 'string') : [];
}

function useRows(branchId: string, seq: number, keys: string[], auto?: boolean) {
  const [data, setData] = useState<Record<string, unknown> | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const load = async () => {
    setBusy(true); setErr(null);
    try { setData((await rows(branchId, seq, keys)).rows); } catch (e) { setErr(errorMessage(e)); }
    setBusy(false);
  };
  useEffect(() => { setData(null); setErr(null); }, [branchId, seq]);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => { if (auto && keys.length) void load(); }, [auto, branchId, seq]);
  return { data, err, busy, load };
}

function RowsAtStep({ branchId, seq, keys, more, auto }: { branchId: string; seq: number; keys: string[]; more: number; auto?: boolean }) {
  const r = useRows(branchId, seq, keys, auto);
  return (
    <Section title="World rows at this step">
      <div className="flex flex-wrap items-center gap-2 text-xs text-muted">
        <Button size="sm" onClick={r.load} disabled={r.busy || keys.length === 0}>{r.busy ? 'Loading…' : 'Rows at this step'}</Button>
        {keys.length === 0 && <span>nothing read or written</span>}
        {more > 0 && <span>(+{more} more not shown)</span>}
      </div>
      {r.err && <ErrorState error={r.err} hint="The saved world for this step could not be read." />}
      {r.data && Object.entries(r.data).map(([k, doc]) => (
        <div key={k} className="flex flex-col gap-0.5">
          <span className="font-mono text-2xs">{k}{doc === null && <span className="text-muted"> (absent)</span>}</span>
          {doc !== null && <Json value={doc} />}
        </div>
      ))}
    </Section>
  );
}

function KeyList({ label, keys }: { label: string; keys: string[] }) {
  return (
    <div className="flex flex-wrap items-baseline gap-1 text-xs">
      <span className="w-12 text-muted">{label}</span>
      {keys.length ? keys.map((k) => <Tag key={k} className="max-w-full truncate font-mono">{k}</Tag>) : <span className="text-faint">none</span>}
    </div>
  );
}

/** A permit's decision: rejected, granted, or not yet decided. */
const permitMark = (approved: boolean | null) =>
  approved === false ? mark('✕', 'Rejected', true) : approved ? mark('◆', 'Granted') : mark('◇', 'Pending');

/** The permit a write ran under (an interlock), or the change it named that approvals never recorded. */
function PermitLine({ events, event }: { events: Event[]; event: Event }) {
  const permit = permitOf(events, event);
  const missing = missingPermit(events, event);
  if (!permit && !missing) return null;
  return (
    <Section title="Permit">
      {permit
        ? <div className="flex flex-wrap items-center gap-1.5 text-xs">
          <StatusMark mark={permitMark(permit.approved)} />
          <span className="font-mono text-2xs">{permit.id}</span><span className="text-muted">{permit.decision || permit.action}</span>
        </div>
        : <StatusMark mark={mark('◇', `no permit: ${missing} is not in approvals`, true)} />}
    </Section>
  );
}

function ToolCall({ branchId, event, events, auto }: { branchId: string; event: Event & { kind: 'tool_call' }; events: Event[]; auto?: boolean }) {
  const p = event.payload;
  const reads = keysOf(p.reads), writes = keysOf(p.writes);
  const { keys, more } = rowKeys(writes, reads);
  return (
    <>
      <PermitLine events={events} event={event} />
      {p.error ? <Section title="Error"><Json value={p.error} error /></Section> : null}
      <Block title="Arguments" value={p.args} />
      <Section title="Reads / writes">
        <KeyList label="Reads" keys={reads} /><KeyList label="Writes" keys={writes} />
        {typeof p.snapshot_error === 'string' && <ErrorState error={`Saving after this step failed: ${p.snapshot_error}`} hint="Reruns from this step can't use a save." />}
        {savedLine(p) && <span className="text-xs text-muted">{savedLine(p)}</span>}
      </Section>
      <RowsAtStep branchId={branchId} seq={event.seq} keys={keys} more={more} auto={auto} />
      {!p.error && <Block title="Result" value={p.result} />}
    </>
  );
}

function messageText(m: unknown): string {
  const c = (m as { content?: unknown })?.content;
  return typeof c === 'string' ? c : pretty(c ?? m);
}

/** "OpenAI · gpt-x": who answered a model call (absent in calls recorded before providers: OpenRouter). */
export function modelCallLine(p: { provider?: string; request?: unknown }): string {
  const model = (p.request as { model?: unknown } | null)?.model;
  return `${providerName(providerId(p.provider))}${typeof model === 'string' && model ? ` · ${model}` : ''}`;
}

function ModelCall({ p }: { p: ModelCallPayload }) {
  const msgs: unknown[] = Array.isArray(asRecord(p.request).messages) ? (asRecord(p.request).messages as unknown[]) : [];
  return (
    <>
      <Section title="Provider"><span className="font-mono text-xs">{modelCallLine(p)}</span></Section>
      <Section title={`Messages (${msgs.length})`}>
        {msgs.length === 0 && <span className="text-xs text-faint">The request was not recorded.</span>}
        {msgs.map((m, i) => (
          <details key={i} open={i === msgs.length - 1} className="text-xs">
            <summary className="cursor-pointer font-mono text-muted">{(m as { role?: string })?.role ?? '?'}</summary>
            <Json value={messageText(m)} />
          </details>
        ))}
      </Section>
      <Block title="Response" value={p.response} />
      <Block title="Usage" value={p.usage} />
      <Section title="Cost"><span className="text-xs">{formatCost(p.cost_usd, p.cost_source, costIsUnknown(providerId(p.provider)))}</span></Section>
      {p.provider_response !== undefined && <Block title="Provider's own response" value={p.provider_response} />}
    </>
  );
}

function Check({ p }: { p: CheckPayload }) {
  const checks = Array.isArray(p.checks) ? p.checks : [];
  const diff = keysOf(p.differing_rows);
  return (
    <>
      <Section title="Outcome"><StatusMark mark={passMark(!!p.passed)} /></Section>
      {checks.length > 0 && (
        <Section title={`Checks (${checks.filter((c) => c.passed).length}/${checks.length})`}>
          <ul className="flex flex-col gap-0.5">{checks.map((c) => (
            <li key={c.name} className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs" title={c.detail}>
              <StatusMark mark={passMark(c.passed, ['Pass', 'Fail'])} />
              <span className="font-mono text-2xs">{c.name}</span>{c.safety && <Tag tone={c.passed ? undefined : 'alarm'}>◆ safety</Tag>}
              {!c.passed && c.detail && <span className="w-full pl-6 text-2xs text-muted">{c.detail}</span>}
            </li>
          ))}</ul>
        </Section>
      )}
      {diff.length > 0 && <Block title="Differing rows" value={diff} />}
      {notComparedLine(keysOf(p.unchecked), 'check') && <Section title="Not compared"><span className="text-xs">{notComparedLine(keysOf(p.unchecked), 'check')}</span></Section>}
      {typeof p.gold_db === 'string' && <Section title="Gold"><span className="font-mono text-2xs text-muted">gold db {p.gold_db}</span></Section>}
    </>
  );
}

/** True when the agent is on a provider that reports no price (OpenAI, Anthropic). */
function agentCostUnknown(team: Team | undefined, agent: string | null): boolean {
  const a = team?.agents.find((x) => x.id === agent);
  return !!a && a.kind === 'open' && costIsUnknown(providerOf(a));
}

function AgentEnd({ p, team, agent }: { p: AgentEndPayload; team?: Team; agent: string | null }) {
  const ok = p.ok !== false;
  const bits = [p.seconds != null ? `${Number(p.seconds).toFixed(1)} s` : '', p.tokens != null ? `${p.tokens} tokens` : '',
    formatCost(p.cost_usd, p.cost_source, agentCostUnknown(team, agent))].filter(Boolean);
  return (
    <Section title="Outcome">
      <span className="flex flex-wrap items-center gap-2 text-xs">
        <StatusMark mark={ok ? MARK.finished : MARK.failed} />
        {typeof p.error === 'string' && p.error && <span className="text-alarm">{p.error}</span>}
      </span>
      <span className="text-xs text-muted">{bits.join(' · ')}</span>
    </Section>
  );
}

/** Why a dispatch run woke the agent, the steps whose mail it got, and the input it was given. */
function Wake({ p }: { p: WakePayload }) {
  const mail = Array.isArray(p.mail) ? p.mail.map(String) : [];
  const title = `${wakeWord(p)}${mail.length ? ` (step ${mail.join(', ')})` : ''}: its input`;
  return <Block title={title} value={p.input} />;
}

/** A dispatch run's steps in words: delegations, reports, wakes, waits and the lead's finish; null for other kinds. */
function dispatchStep(e: Event): ReactElement | null {
  switch (e.kind) {
    case 'delegation': return <Block title={`Work given to ${String(e.payload.to)}`} value={e.payload.task} />;
    case 'report': return <Block title={e.payload.turn_end ? `What it said at the end of its turn, passed on to ${String(e.payload.to)} as its report` : `Report to ${String(e.payload.to)}`} value={e.payload.text} />;
    case 'wake': return <Wake p={e.payload} />;
    case 'idle': return <Block title={e.payload.ok === false ? `Its turn failed; it waits: ${String(e.payload.error ?? '')}` : 'Ended its turn and waits for work; it said'} value={e.payload.text} />;
    case 'finish': return <Block title="Finished the run (only the lead can): every agent stopped and the world was scored" value={e.payload.summary} />;
    default: return null;
  }
}

const SOURCE: Record<string, string> = { agent: 'reported by its code', gateway: "from the model's answer", claude_code: "from Claude Code's stream" };

/** An agent's reasoning in full, where it came from, and the model call it belongs to. */
function Reasoning({ p }: { p: ReasoningPayload }) {
  const call = typeof p.model_call_seq === 'number' ? `, model call at step ${p.model_call_seq}` : '';
  const from = SOURCE[String(p.source)] ?? String(p.source ?? '');
  return (
    <Section title={`${reasoningWord(p.kind)} (${from}${call})`}>
      <p className="whitespace-pre-wrap break-words text-sm">{String(p.text ?? '')}</p>
      {p.truncated === true && <p className="mt-1 text-xs text-muted">Cut at 32,768 characters.</p>}
    </Section>
  );
}

export interface InspectorProps {
  branchId: string;
  event: Event | null;
  team?: Team;
  autoRows?: boolean;
  /** The branch's events, for the system an action touched and the permit it ran under. */
  events?: Event[];
}

function Body({ branchId, event, team, autoRows, events = [] }: InspectorProps & { event: Event }) {
  switch (event.kind) {
    case 'tool_call': return <ToolCall branchId={branchId} event={event} events={events} auto={autoRows} />;
    case 'model_call': return <ModelCall p={event.payload} />;
    case 'handoff': return <Block title={event.payload.to ? `Handoff note for ${String(event.payload.to)}` : 'Handoff note'} value={event.payload.note} />;
    case 'message': return <Block title={`Message to ${event.payload.to === '*' ? 'everyone' : String(event.payload.to)}`} value={event.payload.text} />;
    case 'check': return <Check p={event.payload} />;
    case 'reasoning': return <Reasoning p={event.payload} />;
    case 'agent_end': return <AgentEnd p={event.payload} team={team} agent={event.agent} />;
    default: return dispatchStep(event) ?? <Block title="Payload" value={asRecord(event.payload)} />;
  }
}

/** The step's header: its number, kind, agent and, for an action, the system it touched and how. */
function Head({ event, events = [] }: { event: Event; events?: Event[] }) {
  const a = classify(event, worldInfo(events));
  return (
    <div className="flex flex-col gap-1 px-3 py-2">
      <div className="flex flex-wrap items-center gap-1.5">
        <StepLabel seq={event.seq} kind={event.kind} className="text-sm" />
        {event.agent && <span className="text-xs text-muted">{event.agent}</span>}
        {a && <span className="font-mono text-xs">{a.tool}</span>}
        {a && a.outcome !== 'ok' && <StatusMark mark={alarmMark(a.outcome)} />}
      </div>
      {a && (
        <div className="flex flex-wrap items-center gap-1.5 text-xs text-muted">
          <Tag className={a.effect === 'write' ? 'border-line-strong font-semibold text-fg' : ''}>{actionWord(a)}</Tag>
          {a.part && <span className="font-mono text-2xs">{a.part}</span>}<span className="truncate">{systemLine(a.system)}</span>
        </div>
      )}
    </div>
  );
}

/** One step in full: what it did, on which system, under which permit, and what it returned. */
export default function Inspector(props: InspectorProps) {
  const { event } = props;
  if (!event) {
    return <EmptyState title="Pick a step on the timeline" />;
  }
  return (
    <div className="min-w-0">
      <Head event={event} events={props.events} />
      <Body {...props} event={event} />
    </div>
  );
}
