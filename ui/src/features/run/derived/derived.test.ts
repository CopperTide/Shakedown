// The run page's derived views, from recorded events only: moments that needed a human, and how
// the agents coordinated.
import { describe, expect, it } from 'vitest';
import type { Event } from '@/model/types';
import { coordination } from './coordination';
import { needsAHuman } from './humans';

let seq = 0;
const ev = (kind: string, agent: string | null, payload: Record<string, unknown>): Event =>
  ({ branch_id: 'b', seq: ++seq, at: '2026-11-21T10:00:00Z', agent, kind, payload } as unknown as Event);

const team = [ev('agent_start', 'lead', { name: 'lead' }), ev('agent_start', 'buyer', { name: 'buyer' })];

describe('derived views', () => {
  it('refusal_needing_approval_is_a_moment', () => {
    const events = [...team, ev('tool_call', 'buyer', { name: 'erp.create_po', error: 'refused: a purchase over $150 needs approval', args: {}, result: null })];
    const moments = needsAHuman(events);
    expect(moments).toHaveLength(1);
    expect(moments[0]).toMatchObject({ kind: 'approval', agent: 'buyer' });
  });

  it('message_to_person_is_a_moment', () => {
    const events = [...team, ev('message', 'lead', { to: 'buyer', text: 'find stock' }), ev('message', 'lead', { to: 'planner', text: 'SO-1001 is late' })];
    const moments = needsAHuman(events);
    expect(moments.map((m) => m.kind)).toEqual(['message_to_person']);
    expect(moments[0].text).toContain('planner');
  });

  it('coordination_counts_pairs', () => {
    const events = [...team, ev('delegation', 'lead', { to: 'buyer', task: 'buy' }), ev('message', 'buyer', { to: 'lead', text: 'refused' }), ev('report', 'buyer', { to: 'lead', text: 'done' })];
    const c = coordination(events);
    expect(c.agents).toEqual(['lead', 'buyer']);
    expect(c.links).toEqual([
      { from: 'buyer', to: 'lead', messages: 2, handoffs: 0, delegations: 0 },
      { from: 'lead', to: 'buyer', messages: 0, handoffs: 0, delegations: 1 },
    ]);
  });

  it('single_agent_coordination', () => {
    const c = coordination([ev('agent_start', 'solo', { name: 'solo' }), ev('tool_call', 'solo', { name: 'x', error: null })]);
    expect(c.links).toEqual([]);
  });
});
