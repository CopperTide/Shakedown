// Previous and next step: the same moves as ←/→, as buttons beside the step on screen.
import { ChevronLeft, ChevronRight } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { type Nav, moveIn } from '@/lib/keys';

/** Where the selection sits in `order`: "12 of 340", or how many steps there are when none is picked. */
export function placeLine(order: number[], selected: number | null): string {
  const i = selected == null ? -1 : order.indexOf(selected);
  return i < 0 ? `${order.length} steps` : `${i + 1} of ${order.length}`;
}

/** ‹ 12 of 340 ›: moves through the steps in `order` (the timeline's order, filtered as it is). */
export default function StepStepper({ order, selected, onSelect }: { order: number[]; selected: number | null; onSelect: (seq: number) => void }) {
  const i = selected == null ? -1 : order.indexOf(selected);
  const go = (nav: Nav) => { const next = moveIn(order, selected, nav); if (next != null) onSelect(next); };
  const step = 'size-7 px-0';
  return (
    <div className="flex items-center gap-0.5">
      <Button variant="ghost" size="sm" className={step} aria-label="Previous step" title="Previous step (←)" disabled={!order.length || i === 0} onClick={() => go('prev')}><ChevronLeft /></Button>
      <span className="min-w-14 text-center text-2xs text-muted tabular-nums">{placeLine(order, selected)}</span>
      <Button variant="ghost" size="sm" className={step} aria-label="Next step" title="Next step (→)" disabled={!order.length || i === order.length - 1} onClick={() => go('next')}><ChevronRight /></Button>
    </div>
  );
}
