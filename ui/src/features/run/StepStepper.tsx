// The selected agent's previous and next step: the same moves as ←/→, as buttons beside the step.
import { ChevronLeft, ChevronRight } from 'lucide-react';
import { Button } from '@/components/ui/button';
import type { LaneRow } from '@/model/lanes';
import { type Nav, moveIn } from '@/lib/keys';
import { laneOf, laneSteps } from './laneSteps';

/** Where the selection sits in `order`: "12 of 340", or how many steps there are when none is picked. */
export function placeLine(order: number[], selected: number | null): string {
  const i = selected == null ? -1 : order.indexOf(selected);
  return i < 0 ? `${order.length} steps` : `${i + 1} of ${order.length}`;
}

/** ‹ Planner · 4 of 37 ›: moves through the selected agent's steps in `order` (the timeline's order, filtered as it is). */
export default function StepStepper({ order, lanes, selected, onSelect }: { order: number[]; lanes: LaneRow[]; selected: number | null; onSelect: (seq: number) => void }) {
  const lane = laneOf(lanes, selected);
  const steps = laneSteps(order, lanes, selected);
  const i = selected == null ? -1 : steps.indexOf(selected);
  const go = (nav: Nav) => { const next = moveIn(steps, selected, nav); if (next != null) onSelect(next); };
  const step = 'size-7 px-0';
  const place = placeLine(steps, selected);
  return (
    <div className="flex min-w-0 items-center gap-0.5">
      <Button variant="ghost" size="sm" className={step} aria-label="Previous step" title="This agent's previous step (←)" disabled={!steps.length || i === 0} onClick={() => go('prev')}><ChevronLeft /></Button>
      <span className="min-w-14 max-w-48 truncate text-center text-2xs text-muted tabular-nums" title={lane ? `${lane.name}: step ${place}` : undefined}>{lane ? `${lane.name} · ${place}` : place}</span>
      <Button variant="ghost" size="sm" className={step} aria-label="Next step" title="This agent's next step (→)" disabled={!steps.length || i === steps.length - 1} onClick={() => go('next')}><ChevronRight /></Button>
    </div>
  );
}
