// How a run's agents worked together: who messaged, handed off to or delegated to whom, counted
// per direction. Read from the recorded events.
import type { Event } from '@/model/types';
import { agentsOf } from './agents';

export interface Link { from: string; to: string; messages: number; handoffs: number; delegations: number }

const FIELD = { message: 'messages', report: 'messages', handoff: 'handoffs', delegation: 'delegations' } as const;

export function coordination(events: Event[]): { agents: string[]; links: Link[] } {
  const agents = agentsOf(events);
  const links = new Map<string, Link>();
  for (const e of events) {
    const field = FIELD[e.kind as keyof typeof FIELD];
    const to = (e.payload as { to?: string } | null)?.to;
    if (!field || !e.agent || !to || !agents.includes(to)) continue;
    const key = `${e.agent}→${to}`;
    const link = links.get(key) ?? { from: e.agent, to, messages: 0, handoffs: 0, delegations: 0 };
    link[field] += 1;
    links.set(key, link);
  }
  return { agents, links: [...links.values()].sort((a, b) => a.from.localeCompare(b.from) || a.to.localeCompare(b.to)) };
}

const KIND: Record<string, string> = { message: 'Message', report: 'Report', delegation: 'Delegation', handoff: 'Handoff' };

/** Recorded exchanges, including broadcasts and messages to agents outside the declared team. */
export function exchangesOf(events: Event[]) {
  return events.filter((e) => Object.hasOwn(KIND, e.kind)).map((e) => {
    const p = e.payload as Record<string, unknown>;
    return {
      seq: e.seq, kind: KIND[e.kind], from: e.agent ?? 'System',
      to: p.to === '*' ? 'Everyone' : String(p.to ?? (e.kind === 'report' ? 'Lead' : 'Next agent')),
      text: String(p.text ?? p.task ?? p.note ?? ''),
    };
  }).sort((a, b) => a.seq - b.seq);
}
