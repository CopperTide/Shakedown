import { describe, expect, it } from 'vitest';
import type { Event, Team } from '@/model/types';
import { agentStats, agentsIn, preview, usageOf } from './agentStats';

const ev = (seq: number, kind: Event['kind'], payload: unknown, agent: string | null = 'ops', sec = seq): Event =>
  ({ branch_id: 'b', seq, at: new Date(Date.UTC(2026, 10, 25, 12, 0, sec)).toISOString(), agent, kind, payload }) as Event;

const openai = (prompt: number, completion: number, cached: number, cost: number | null) => ({
  bodies_omitted: true, request: { model: 'deepseek/v4' }, response: null, cost_usd: cost, cost_source: 'openrouter',
  usage: { prompt_tokens: prompt, completion_tokens: completion, prompt_tokens_details: { cached_tokens: cached }, completion_tokens_details: { reasoning_tokens: 7 } },
});

const team: Team = { id: 't', name: 'T', agents: [
  { id: 'lead', name: 'Lead', kind: 'open', model: 'm', prompt: '' },
  { id: 'ops', name: 'Ops', kind: 'open', model: 'm', prompt: '' },
  { id: 'idle', name: 'Never acted', kind: 'open', model: 'm', prompt: '' },
] };

const run: Event[] = [
  ev(1, 'run_start', { task_id: 'x' }, null),
  ev(2, 'agent_start', { name: 'ops', kind: 'open', model: 'm' }),
  ev(3, 'wake', { reason: 'start', mail: [], input: 'go' }),
  ev(5, 'model_call', openai(1000, 50, 800, 0.01)),
  ev(6, 'reasoning', { text: 'Look first.', kind: 'thinking', source: 'gateway', model_call_seq: 5 }, 'ops', 5),
  ev(7, 'tool_call', { name: 'ops_flights', args: { hub: 'ORD' }, result: { flights: [] }, error: null, duration_ms: 200 }),
  ev(8, 'tool_call', { name: 'ops_cancel', args: { flight: 'UA1' }, result: null, error: 'refused: needs approval', duration_ms: 50 }),
  ev(11, 'model_call', openai(1500, 30, 1000, null)),
  ev(12, 'tool_call', { name: 'crew_swap', args: {}, result: null, error: 'pilot not found' }),
  ev(13, 'idle', { ok: false, text: '', error: 'boom' }),
  ev(14, 'tool_call', { name: 'ops_flights', args: {}, result: 'x', error: null }, 'lead'),
];

describe('agentStats', () => {
  const s = agentStats(run, 'ops');

  it('counts turns, calls, tokens and cost', () => {
    expect(s.turns).toBe(1);
    expect(s.calls.map((c) => c.seq)).toEqual([5, 11]);
    expect(s.tokens).toEqual({ input: 2500, output: 80, cached: 1800, reasoning: 14 });
    expect(s.costUsd).toBeCloseTo(0.01);
    expect(s.uncosted).toBe(1);
  });

  it('estimates thinking from the step before each call, ignoring reasoning records', () => {
    // wake at 3s → call at 5s; tool at 8s → call at 11s.
    expect(s.calls.map((c) => c.thinkMs)).toEqual([2000, 3000]);
    expect(s.thinkMs).toBe(5000);
    expect(s.toolMs).toBe(250);
  });

  it('separates refusals from errors, failed turns included', () => {
    expect(s.refusals).toBe(1);
    expect(s.errors).toBe(2);
    expect(s.tools.map((t) => [t.tool, t.outcome])).toEqual([['ops_flights', 'ok'], ['ops_cancel', 'refused'], ['crew_swap', 'error']]);
    expect(s.tools[1].result).toBe('refused: needs approval');
  });

  it('keeps the reasoning in order, linked to its model call', () => {
    expect(s.reasoning).toEqual([{ seq: 6, kind: 'thinking', text: 'Look first.', modelCallSeq: 5, truncated: false, inherited: false }]);
  });

  it("marks a fork's inherited steps", () => {
    const f = agentStats(run, 'ops', 8);
    expect(f.calls.map((c) => c.inherited)).toEqual([true, false]);
  });

  it('falls back to starts when an agent is never woken', () => {
    expect(agentStats([ev(2, 'agent_start', {}), ev(3, 'model_call', openai(1, 1, 0, 0))], 'ops').turns).toBe(1);
    expect(agentStats(run, 'nobody').calls).toEqual([]);
  });
});

describe('usageOf', () => {
  it('reads Anthropic-style usage as the whole prompt', () => {
    expect(usageOf({ usage: { input_tokens: 10, cache_read_input_tokens: 900, cache_creation_input_tokens: 100, output_tokens: 7 } }))
      .toEqual({ input: 1010, output: 7, cached: 900, reasoning: null });
  });
  it('is null when nothing was recorded', () => {
    expect(usageOf({ usage: null })).toEqual({ input: null, output: null, cached: null, reasoning: null });
  });
});

describe('agentsIn', () => {
  it('lists agents that acted, team order first, then joiners; never the world', () => {
    const events = [...run, ev(20, 'tool_call', { name: 'x' }, 'joiner'), ev(21, 'tool_call', { name: 'x' }, 'link')];
    expect(agentsIn(events, team)).toEqual([{ id: 'lead', name: 'Lead' }, { id: 'ops', name: 'Ops' }, { id: 'joiner', name: 'joiner' }]);
  });
});

describe('preview', () => {
  it('clips compact JSON to one line', () => {
    expect(preview({ a: 'x'.repeat(300) }, 20)).toHaveLength(20);
    expect(preview('two\n  lines')).toBe('two lines');
    expect(preview(null)).toBe('');
  });
});
