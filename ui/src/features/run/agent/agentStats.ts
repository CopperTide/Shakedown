// One agent of a run, from the steps the run page already holds (lean model calls keep usage and
// cost): its figures, its context size per model call, its reasoning in order and its tool calls.
//
//   events ──► agentsIn  (who can be picked)
//          └─► agentStats(events, agent) ──► totals · calls[] (chart) · reasoning[] (trail) · tools[]
//
// Thinking time is an estimate: a model call is recorded when its answer comes back, so the time
// since the agent's previous step is taken as the call's. Tool time is what the world measured.
import { asRecord } from '@/lib/record';
import { outcomeOf, type Outcome } from '@/model/actions';
import { isWorldActor } from '@/model/lanes';
import type { Event, ReasoningPayload, Team } from '@/model/types';

export interface Usage {
  /** The whole prompt (the context the model read), cached tokens included. */
  input: number | null;
  output: number | null;
  cached: number | null;
  reasoning: number | null;
}

export interface CallRow extends Usage {
  seq: number;
  at: number;
  model: string | null;
  costUsd: number | null;
  /** Time since the agent's previous step (null for its first). */
  thinkMs: number | null;
  inherited: boolean;
}

export interface ReasoningRow {
  seq: number;
  kind: ReasoningPayload['kind'];
  text: string;
  modelCallSeq: number | null;
  truncated: boolean;
  inherited: boolean;
}

export interface ToolRow {
  seq: number;
  tool: string;
  args: string;
  result: string;
  outcome: Outcome;
  error: string | null;
  durationMs: number | null;
  inherited: boolean;
}

export interface AgentStats {
  agent: string;
  /** Wakes in a dispatch run, else starts. */
  turns: number;
  tokens: { input: number; output: number; cached: number; reasoning: number };
  /** Known cost, and how many calls had none. */
  costUsd: number | null;
  uncosted: number;
  thinkMs: number;
  /** Null when no tool call recorded its duration. */
  toolMs: number | null;
  refusals: number;
  /** Failed tool calls, failed turns and a failed end. */
  errors: number;
  calls: CallRow[];
  reasoning: ReasoningRow[];
  tools: ToolRow[];
}

const num = (v: unknown): number | null => (typeof v === 'number' && Number.isFinite(v) ? v : null);
const sum = (xs: (number | null)[]) => xs.reduce<number>((n, x) => n + (x ?? 0), 0);

/** A model call's tokens, OpenAI-style (`prompt_tokens`, details) or Anthropic-style (`input_tokens`, cache reads apart). */
export function usageOf(payload: unknown): Usage {
  const p = asRecord(payload);
  const u = asRecord(p.usage);
  const details = asRecord(u.prompt_tokens_details);
  const prompt = num(u.prompt_tokens);
  if (prompt != null) {
    return { input: prompt, output: num(u.completion_tokens), cached: num(details.cached_tokens), reasoning: num(asRecord(u.completion_tokens_details).reasoning_tokens) };
  }
  const read = num(u.cache_read_input_tokens);
  const fresh = num(u.input_tokens) ?? num(p.input_tokens);
  const input = fresh == null && read == null ? null : (fresh ?? 0) + (read ?? 0) + (num(u.cache_creation_input_tokens) ?? 0);
  return { input, output: num(u.output_tokens) ?? num(p.output_tokens), cached: read, reasoning: null };
}

const clip = (s: string, n: number) => (s.length > n ? `${s.slice(0, n - 1)}…` : s);

/** A value as one short line: a string as it is, anything else as compact JSON. */
export function preview(v: unknown, n = 160): string {
  if (v == null) return '';
  const s = typeof v === 'string' ? v : JSON.stringify(v);
  return clip(s.replace(/\s+/g, ' ').trim(), n);
}

/** The run's agents that can be picked: the team's in order, then any other agent that recorded a step. */
export function agentsIn(events: Event[], team: Team): { id: string; name: string }[] {
  const acted = new Set(events.map((e) => e.agent).filter((a): a is string => !isWorldActor(a)));
  const listed = team.agents.filter((a) => acted.has(a.id)).map((a) => ({ id: a.id, name: a.name }));
  const others = [...acted].filter((id) => !team.agents.some((a) => a.id === id)).map((id) => ({ id, name: id }));
  return [...listed, ...others];
}

function callRow(e: Event, prevAt: number | null, inherited: boolean): CallRow {
  const p = asRecord(e.payload);
  const at = Date.parse(e.at);
  const model = asRecord(p.request).model ?? p.model;
  return {
    seq: e.seq, at, ...usageOf(p), inherited,
    model: typeof model === 'string' ? model : null,
    costUsd: num(p.cost_usd),
    thinkMs: prevAt == null || Number.isNaN(at) ? null : Math.max(0, at - prevAt),
  };
}

function toolRow(e: Event, inherited: boolean): ToolRow {
  const p = asRecord(e.payload);
  const error = p.error == null ? null : String(p.error);
  return {
    seq: e.seq, tool: String(p.name ?? 'tool'), args: preview(p.args), result: error ? preview(error) : preview(p.result),
    outcome: outcomeOf(error), error, durationMs: num(p.duration_ms), inherited,
  };
}

function reasoningRow(e: Event, inherited: boolean): ReasoningRow {
  const p = asRecord(e.payload);
  return {
    seq: e.seq, kind: (p.kind as ReasoningPayload['kind']) ?? 'thinking', text: String(p.text ?? ''),
    modelCallSeq: num(p.model_call_seq), truncated: p.truncated === true, inherited,
  };
}

/** Steps that don't move an agent's clock: reasoning is written with its model call, a save point by shaked. */
const PASSIVE = new Set<Event['kind']>(['reasoning', 'save_point', 'world_view', 'spans']);

interface Walk { calls: CallRow[]; reasoning: ReasoningRow[]; tools: ToolRow[] }

/** The agent's steps in order, sorted into model calls (each timed from the step before it), reasoning and tool calls. */
function walk(mine: Event[], startSeq: number | null): Walk {
  const w: Walk = { calls: [], reasoning: [], tools: [] };
  let prevAt: number | null = null;
  for (const e of mine) {
    const inherited = startSeq != null && e.seq < startSeq;
    if (e.kind === 'model_call') w.calls.push(callRow(e, prevAt, inherited));
    else if (e.kind === 'reasoning') w.reasoning.push(reasoningRow(e, inherited));
    else if (e.kind === 'tool_call') w.tools.push(toolRow(e, inherited));
    const at = Date.parse(e.at);
    if (!PASSIVE.has(e.kind) && !Number.isNaN(at)) prevAt = at;
  }
  return w;
}

const count = (events: Event[], test: (e: Event) => boolean) => events.reduce((n, e) => n + (test(e) ? 1 : 0), 0);
const failedTurn = (e: Event) => (e.kind === 'idle' || e.kind === 'agent_end') && asRecord(e.payload).ok === false;

/** Everything the agent inspector shows of `agent`, over the branch's whole record (a fork's inherited steps marked). */
export function agentStats(events: Event[], agent: string, startSeq: number | null = null): AgentStats {
  const mine = events.filter((e) => e.agent === agent).sort((a, b) => a.seq - b.seq);
  const { calls, reasoning, tools } = walk(mine, startSeq);
  const timed = tools.filter((t) => t.durationMs != null);
  const costed = calls.filter((c) => c.costUsd != null);
  return {
    agent,
    turns: count(mine, (e) => e.kind === 'wake') || count(mine, (e) => e.kind === 'agent_start'),
    tokens: { input: sum(calls.map((c) => c.input)), output: sum(calls.map((c) => c.output)), cached: sum(calls.map((c) => c.cached)), reasoning: sum(calls.map((c) => c.reasoning)) },
    costUsd: costed.length ? sum(costed.map((c) => c.costUsd)) : null,
    uncosted: calls.length - costed.length,
    thinkMs: sum(calls.map((c) => c.thinkMs)),
    toolMs: timed.length ? sum(timed.map((t) => t.durationMs)) : null,
    refusals: tools.filter((t) => t.outcome === 'refused').length,
    errors: tools.filter((t) => t.outcome === 'error').length + count(mine, failedTurn),
    calls, reasoning, tools,
  };
}
