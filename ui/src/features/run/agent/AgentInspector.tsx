// The run page's "Agent" tab: pick one agent and see its figures, its context size over its
// model calls, its reasoning in order and its tool calls. Every item selects its step, so the
// side Inspector shows the exact context at that moment. Derived from the steps already loaded.
import { type ReactNode, useMemo, useState } from 'react';
import { Hint } from '@/components/ui/hint';
import { Stat } from '@/components/ui/page';
import { cn } from '@/lib/cn';
import { duration, usd } from '@/lib/format';
import type { Event, Team } from '@/model/types';
import { type AgentStats, agentStats, agentsIn } from './agentStats';
import ContextChart from './ContextChart';
import ToolCalls from './ToolCalls';
import Trail from './Trail';

const tok = (n: number) => n.toLocaleString();

function Summary({ s }: { s: AgentStats }) {
  const cost = s.costUsd == null ? 'unknown' : `${usd(s.costUsd)}${s.uncosted ? ` + ${s.uncosted} unpriced` : ''}`;
  return (
    <div className="flex flex-wrap gap-x-8 gap-y-2 px-3 py-2.5">
      <Stat label="Turns" value={s.turns} />
      <Stat label="Model calls" value={s.calls.length} />
      <Stat label="Tokens in · out" value={`${tok(s.tokens.input)} · ${tok(s.tokens.output)}`} />
      <Stat label="Cached in" value={`${tok(s.tokens.cached)}${s.tokens.input ? ` (${Math.round((s.tokens.cached / s.tokens.input) * 100)}%)` : ''}`} />
      {s.tokens.reasoning > 0 && <Stat label="Reasoning tokens" value={tok(s.tokens.reasoning)} />}
      <Stat label="Cost" value={cost} />
      <Stat label="Thinking (est.)" value={duration(s.thinkMs / 1000)} title="Time from the agent's previous step to each model call's answer" />
      <Stat label="In tools" value={s.toolMs == null ? '–' : duration(s.toolMs / 1000)} title="Time the world measured running its tool calls" />
      <Stat label="Tool calls" value={s.tools.length} />
      <Stat label="Refusals" value={s.refusals} abnormal={s.refusals > 0} />
      <Stat label="Errors" value={s.errors} abnormal={s.errors > 0} />
    </div>
  );
}

function Section({ title, hint, meta, children }: { title: string; hint?: string; meta?: ReactNode; children: ReactNode }) {
  return (
    <section className="border-t border-line">
      <h3 className="flex items-baseline gap-2 px-3 pt-2 text-xs font-semibold text-muted"><Hint hint={hint}>{title}</Hint>{meta != null && <span className="font-normal text-faint">{meta}</span>}</h3>
      {children}
    </section>
  );
}

function Picker({ agents, value, onChange }: { agents: { id: string; name: string }[]; value: string; onChange: (id: string) => void }) {
  return (
    <div role="radiogroup" aria-label="Agent" className="flex flex-wrap gap-1 px-3 py-2">
      {agents.map((a) => (
        <button key={a.id} type="button" role="radio" aria-checked={a.id === value} onClick={() => onChange(a.id)}
          className={cn('rounded-md border px-2 py-1 text-xs', a.id === value ? 'border-accent bg-accent-soft text-fg' : 'border-line text-muted hover:bg-hover hover:text-fg')}>
          {a.name}
        </button>
      ))}
    </div>
  );
}

/** How to read the context chart; a rerun's dimmed bars are the steps it carried over. */
function contextHint(forkSeq: number | null): string {
  const base = 'Input tokens per model call, first call to last; the lighter part was cached. Click a bar to open its step.';
  return forkSeq == null ? base : `${base}\nDimmed: steps before the fork at step ${forkSeq}, carried over by this rerun.`;
}

export interface AgentInspectorProps {
  events: Event[];
  team: Team;
  startSeq: number | null;
  selected: number | null;
  onSelect: (seq: number) => void;
}

export default function AgentInspector({ events, team, startSeq, selected, onSelect }: AgentInspectorProps) {
  const agents = useMemo(() => agentsIn(events, team), [events, team]);
  const selectedAgent = events.find((e) => e.seq === selected)?.agent ?? null;
  const [picked, setPicked] = useState<string | null>(null);
  const agent = [picked, selectedAgent].find((a) => a != null && agents.some((x) => x.id === a)) ?? agents[0]?.id ?? null;
  const s = useMemo(() => (agent ? agentStats(events, agent, startSeq) : null), [events, agent, startSeq]);
  if (!agent || !s) return <p className="px-3 py-6 text-sm text-muted">No agent has acted yet</p>;
  const inherited = startSeq != null && s.calls.some((c) => c.inherited);
  return (
    <div className="flex flex-col">
      <Picker agents={agents} value={agent} onChange={setPicked} />
      <Summary s={s} />
      <Section title="Context size" hint={contextHint(inherited ? startSeq : null)}><ContextChart calls={s.calls} selected={selected} onSelect={onSelect} /></Section>
      <Section title="Reasoning" meta={s.reasoning.length.toLocaleString()}><Trail rows={s.reasoning} agent={agent} selected={selected} onSelect={onSelect} /></Section>
      <Section title="Tool calls" meta={s.tools.length.toLocaleString()}><ToolCalls rows={s.tools} agent={agent} selected={selected} onSelect={onSelect} /></Section>
    </div>
  );
}
