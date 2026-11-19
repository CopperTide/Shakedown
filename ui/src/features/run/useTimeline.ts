import { useMemo } from 'react';
import { buildLanes, spanOf } from '@/model/lanes';
import type { BranchDetail } from '@/model/types';
import { stepOrder } from './timelineLayout';

/** A branch's timeline: its lanes, their time span, and the order ←/→ and j/k move through (model calls only with `models`). */
export function useTimeline(d: BranchDetail | null, models: boolean) {
  const lanes = useMemo(() => (d ? buildLanes(d.events, d.team, d.start_seq) : []), [d]);
  const order = useMemo(() => stepOrder(lanes, models), [lanes, models]);
  return { lanes, span: spanOf(lanes), order };
}

export type Timeline = ReturnType<typeof useTimeline>;
