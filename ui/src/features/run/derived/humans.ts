// Moments in a run that needed a human: calls refused until a person approves, and messages to
// someone who isn't one of the run's agents. Read from the recorded events; nothing is guessed.
import type { Event } from '@/model/types';
import { agentsOf } from './agents';

export interface Moment {
  seq: number;
  at: string;
  agent: string | null;
  kind: 'approval' | 'message_to_person';
  text: string;
}

const NEEDS_PERSON = /approv|needs a person|human/i;

export function needsAHuman(events: Event[]): Moment[] {
  const agents = new Set(agentsOf(events));
  const out: Moment[] = [];
  for (const e of events) {
    const base = { seq: e.seq, at: e.at, agent: e.agent };
    if (e.kind === 'tool_call') {
      const p = e.payload as { name: string; error: string | null };
      if (p.error && NEEDS_PERSON.test(p.error)) out.push({ ...base, kind: 'approval', text: `${p.name}: ${p.error}` });
    } else if (e.kind === 'message') {
      const p = e.payload as { to: string; text: string };
      if (!agents.has(p.to)) out.push({ ...base, kind: 'message_to_person', text: `To ${p.to}: ${p.text}` });
    }
  }
  return out;
}
