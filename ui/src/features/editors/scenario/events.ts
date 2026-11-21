// Scenario events: when each fires (a world time or a number of agent calls, never both), and the
// order they fire in. Pure.
import type { ScenarioEvent } from '@/model/project';

export type Timing = 'time' | 'calls';

/** `event` firing at world time `value`, or after `value` agent calls. */
export function setTiming(event: ScenarioEvent, timing: Timing, value: string): ScenarioEvent {
  const { at: _at, after_calls: _calls, ...rest } = event;
  return timing === 'time' ? { ...rest, at: value } : { ...rest, after_calls: Number(value) };
}

/** Events with a world time, earliest first, and the ones that fire after agent calls. */
export function timeline(events: ScenarioEvent[]): { timed: ScenarioEvent[]; byCalls: ScenarioEvent[] } {
  const timed = events.filter((e) => e.at != null).sort((a, b) => (a.at ?? '').localeCompare(b.at ?? ''));
  return { timed, byCalls: events.filter((e) => e.at == null) };
}
