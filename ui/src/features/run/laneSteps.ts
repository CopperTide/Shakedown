// One agent's steps: what ←/→ and the step buttons move through, so stepping stays on the selected
// step's lane. ↑/↓ still move through the whole run's steps in order.
import type { LaneRow } from '@/model/lanes';

/** The lane step `seq` is on, or null when no step is picked or none has it. */
export function laneOf(lanes: LaneRow[], seq: number | null): LaneRow | null {
  if (seq == null) return null;
  return lanes.find((l) => l.marks.some((m) => m.seq === seq)) ?? null;
}

/** The steps of `order` on `seq`'s lane; all of `order` when `seq` has no lane. */
export function laneSteps(order: number[], lanes: LaneRow[], seq: number | null): number[] {
  const lane = laneOf(lanes, seq);
  if (!lane) return order;
  const mine = new Set(lane.marks.map((m) => m.seq));
  return order.filter((s) => mine.has(s));
}
