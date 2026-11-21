// Edits to a world's observability section. Pure.
import type { HumanMoment, Observability, OperatorBehavior, WorldBody } from '@/model/project';

export const MOMENTS: { moment: HumanMoment; label: string }[] = [
  { moment: 'paged', label: 'When someone is paged' },
  { moment: 'approval', label: 'When an approval is asked for' },
  { moment: 'gate_needs_person', label: 'When a gate needs a person' },
  { moment: 'agent_stuck', label: 'When an agent is stuck' },
  { moment: 'message_to_person', label: 'When an agent messages a person' },
];

/** `world` with its observability section replaced; an empty section is removed. */
export function withObservability(world: WorldBody, obs: Observability): WorldBody {
  const kept = Object.fromEntries(Object.entries(obs).filter(([, v]) => Array.isArray(v) && v.length > 0)) as Observability;
  const { observability: _old, ...rest } = world;
  return Object.keys(kept).length ? { ...rest, observability: kept } : rest;
}

/** The behavior the form chose: a word, or answering after some minutes. */
export function behaviorOf(choice: string, minutes: string): OperatorBehavior {
  if (choice === 'answers_after') return { answers_after: { minutes: Math.max(0, Number(minutes) || 0) } };
  return choice as OperatorBehavior;
}

/** `rules` with `moment` set to `then`, or removed when `then` is empty. */
export function setRule(rules: Observability['human'], moment: HumanMoment, then: string): NonNullable<Observability['human']> {
  const others = (rules ?? []).filter((r) => r.moment !== moment);
  return then ? [...others, { moment, then: then as 'continue' | 'pause' | 'fail' }] : others;
}
