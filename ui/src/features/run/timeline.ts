import { usd as dollars } from '@/lib/format';
import { asRecord } from '@/lib/record';
import { payloadOf } from '@/model/events';
import { LOCAL_COST_SOURCE, providerId } from '@/model/providers';
import type { AgentKind, AgentSpec, Branch, Change, Event } from '@/model/types';

export function costLabel(source: string | null | undefined): string {
  if (source === 'api_equivalent_estimate') return 'API-equivalent estimate';
  if (source === 'openrouter') return 'OpenRouter';
  if (source === LOCAL_COST_SOURCE) return 'no API charge';
  if (source === 'mixed') return 'mixed';
  return source ? 'reported' : '';
}

/**
 * A cost with its source. No dollar figure reads "unknown" when an OpenAI or Anthropic call is in
 * it (`unknown`: they report no price), else "tokens only" (Codex); a local server's cost reads
 * "no API charge".
 */
export function formatCost(usd: number | null | undefined, source: string | null | undefined, unknown = false): string {
  if (usd == null) return unknown ? 'unknown' : 'tokens only';
  if (usd === 0 && source === LOCAL_COST_SOURCE) return 'no API charge';
  const l = costLabel(source);
  return `${dollars(usd)}${l ? ` (${l})` : ''}`;
}

export interface TimelineSummary {
  seconds: number | null;
  live: boolean;
  tokens: number;
  cost_usd: number | null;
  cost_source: string | null;
  /** "since fork at step N" for a forked branch, else null. */
  scope: string | null;
}

/** Same arithmetic as the server's compare summary: own events only (seq >= start_seq). */
export function summarize(
  branch: Pick<Branch, 'start_seq' | 'created_at' | 'ended_at' | 'status'>,
  events: Event[],
  now: number = Date.now(),
): TimelineSummary {
  const spend = ownSpend(events, branch.start_seq ?? 0);
  const time = elapsed(branch, now);
  return {
    ...time, tokens: spend.tokens, cost_usd: spend.cost,
    cost_source: spend.sources.length === 0 ? null : spend.sources.length === 1 ? spend.sources[0] : 'mixed',
    scope: branch.start_seq != null ? `since fork at step ${branch.start_seq}` : null,
  };
}

/**
 * Tokens and cost of the branch's own events: each agent's `agent_end` once it has ended, and the
 * model calls so far of an agent still running (so a live run doesn't read $0). The cost is unknown
 * once any part is.
 */
function ownSpend(events: Event[], from: number): { tokens: number; cost: number | null; sources: string[] } {
  const own = events.filter((e) => e.seq >= from);
  const ended = new Set(own.filter((e) => e.kind === 'agent_end').map((e) => e.agent));
  let tokens = 0;
  let cost: number | null = 0;
  const sources: string[] = [];
  const add = (t: number, c: number | null | undefined, source: string | null | undefined) => {
    tokens += t;
    cost = cost != null && c != null ? cost + c : null;
    if (source && !sources.includes(source)) sources.push(source);
  };
  for (const e of own) {
    const end = payloadOf(e, 'agent_end');
    if (end) { add(end.tokens ?? 0, end.cost_usd, end.cost_source); continue; }
    if (e.kind !== 'model_call' || ended.has(e.agent)) continue;
    const p = asRecord(e.payload);
    const usage = asRecord(p.usage);
    add(typeof usage.total_tokens === 'number' ? usage.total_tokens : 0,
      typeof p.cost_usd === 'number' ? p.cost_usd : null, typeof p.cost_source === 'string' ? p.cost_source : null);
  }
  return { tokens, cost, sources };
}

/** Seconds from the branch's start to its end, or to now while it runs. */
function elapsed(branch: Pick<Branch, 'created_at' | 'ended_at' | 'status'>, now: number): { seconds: number | null; live: boolean } {
  const start = Date.parse(branch.created_at);
  const live = !branch.ended_at && branch.status === 'running';
  const end = branch.ended_at ? Date.parse(branch.ended_at) : live ? now : NaN;
  return { seconds: Number.isNaN(start) || Number.isNaN(end) ? null : (end - start) / 1000, live };
}

/**
 * Only fields that really changed. Prompts compare trimmed. A cleared model that was set is sent as ""
 * (explicit clear); blank on an unset model is no change. A mode change resets the model server-side,
 * so a blank model box next to it sends just the mode. A provider (API mode only) is sent only when
 * it differs from the agent's (absent is `openrouter`), never blank, and always with its model: the
 * old model belongs to the old provider.
 */
export function changedFields(agents: AgentSpec[], edits: Record<string, Change>): Record<string, Change> {
  const out: Record<string, Change> = {};
  for (const a of agents) {
    const c = edits[a.id] ? agentChange(a, edits[a.id]) : {};
    if (Object.keys(c).length) out[a.id] = c;
  }
  return out;
}

/** One agent's real changes (see `changedFields`). */
function agentChange(a: AgentSpec, e: Change): Change {
  const c: Change = {};
  if (e.prompt != null && e.prompt.trim() !== a.prompt.trim()) c.prompt = e.prompt.trim();
  const kindChanged = e.kind != null && e.kind !== (a.kind as AgentKind);
  const open = (e.kind ?? a.kind) === 'open';
  const providerChanged = open && e.provider != null && providerId(e.provider) !== providerId(a.provider);
  if (providerChanged) Object.assign(c, { provider: providerId(e.provider), model: e.model?.trim() ?? '' });
  else Object.assign(c, modelChange(a, e.model, kindChanged));
  if (kindChanged) c.kind = e.kind;
  return c;
}

/** A model edit with the provider unchanged: sent when it differs, a blank next to a mode change dropped. */
function modelChange(a: AgentSpec, model: string | null | undefined, kindChanged: boolean): Change {
  const m = model?.trim();
  if (m == null || m === (a.model ?? '') || (m === '' && kindChanged)) return {};
  return { model: m };
}

export const MAX_ROW_KEYS = 50;

/** Row keys to fetch for a step: writes first, wildcard reads (`table/*`) dropped, capped. */
export function rowKeys(writes: string[], reads: string[]): { keys: string[]; more: number } {
  const all = [...new Set([...writes, ...reads])].filter((k) => !k.endsWith('/*'));
  return { keys: all.slice(0, MAX_ROW_KEYS), more: Math.max(0, all.length - MAX_ROW_KEYS) };
}
