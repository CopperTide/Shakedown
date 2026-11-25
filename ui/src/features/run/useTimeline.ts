import { useMemo } from 'react';
import { onForkClock } from '@/model/forkTime';
import { buildLanes, spanOf } from '@/model/lanes';
import type { BranchDetail } from '@/model/types';
import { stepOrder } from './timelineLayout';

/**
 * A branch's timeline: its lanes, their time span, and the order ←/→ and j/k move through (model
 * calls only with `models`). A fork's own steps are drawn on the fork's clock (`onForkClock`).
 */
export function useTimeline(d: BranchDetail | null, models: boolean) {
  const lanes = useMemo(() => (d ? buildLanes(onForkClock(d.events, d.start_seq), d.team, d.start_seq) : []), [d]);
  const order = useMemo(() => stepOrder(lanes, models), [lanes, models]);
  const span = useMemo(() => spanOf(lanes), [lanes]);
  return { lanes, span, order };
}

export type Timeline = ReturnType<typeof useTimeline>;
