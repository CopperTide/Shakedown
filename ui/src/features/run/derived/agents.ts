// The agents of a run, in the order they started.
import type { Event } from '@/model/types';

export function agentsOf(events: Event[]): string[] {
  const seen: string[] = [];
  for (const e of events) {
    const name = e.kind === 'agent_start' ? (e.payload as { name?: string }).name ?? e.agent : null;
    if (name && !seen.includes(name)) seen.push(name);
  }
  return seen;
}
