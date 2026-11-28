import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { Event, Team } from '@/model/types';
import AgentInspector from './AgentInspector';

const team: Team = { id: 't', name: 'T', agents: [{ id: 'ops', name: 'Ops', kind: 'open', model: 'm', prompt: '' }, { id: 'crew', name: 'Crew', kind: 'open', model: 'm', prompt: '' }] };
const ev = (seq: number, kind: Event['kind'], payload: unknown, agent = 'ops'): Event =>
  ({ branch_id: 'b', seq, at: new Date(Date.UTC(2026, 10, 25, 12, 0, 0, seq * 10)).toISOString(), agent, kind, payload }) as Event;

// A long run: 300 model calls, each with a reasoning step; the crew agent acts once.
const events: Event[] = [ev(1, 'agent_start', {}), ev(2, 'tool_call', { name: 'x', args: {}, result: null, error: null }, 'crew')];
for (let i = 0; i < 300; i++) {
  const seq = 10 + i * 2;
  events.push(ev(seq, 'model_call', { usage: { prompt_tokens: 1000 + i, completion_tokens: 5 }, cost_usd: 0.001 }));
  events.push(ev(seq + 1, 'reasoning', { text: `thought **${i}**`, kind: 'thinking', source: 'gateway', model_call_seq: seq }));
}

describe('agent inspector', () => {
  const html = renderToStaticMarkup(<AgentInspector events={events} team={team} startSeq={null} selected={null} onSelect={() => {}} />);

  it('offers every agent that acted and opens on the first', () => {
    expect(html).toContain('aria-checked="true"');
    expect(html.indexOf('>Ops<')).toBeLessThan(html.indexOf('>Crew<'));
  });

  it('draws a page of the reasoning trail, not all of it', () => {
    expect(html.match(/<strong class="font-semibold">/g)).toHaveLength(20);
    expect(html).toContain('Showing 20 of 300');
    expect(html).toContain('context · model call 10');
  });

  it('charts one bar per model call', () => {
    expect(html.match(/<title>Step \d+: /g)).toHaveLength(300);
  });

  it("follows the selected step's agent", () => {
    const crew = renderToStaticMarkup(<AgentInspector events={events} team={team} startSeq={null} selected={2} onSelect={() => {}} />);
    expect(crew).toContain('No reasoning');
  });
});
