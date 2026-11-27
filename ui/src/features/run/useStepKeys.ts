import { useEffect } from 'react';
import type { LaneRow } from '@/model/lanes';
import { hasModifier, isTyping, moveIn, navOf } from '@/lib/keys';
import { laneSteps } from './laneSteps';

/**
 * ←/→ (h/l) move the selection to the selected agent's previous or next step; ↑/↓ (j/k), Home and
 * End move through all the steps in `order`. Not while a field, dialog, menu, handle or tab (whose
 * arrows switch tabs) has the keys.
 */
export function useStepKeys(order: number[], lanes: LaneRow[], setSelected: (f: (s: number | null) => number | null) => void) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (isTyping(e.target) || hasModifier(e) || (e.target as HTMLElement).closest?.('[role="dialog"],[role="separator"],[role="menu"],[role="tab"]')) return;
      const nav = navOf(e.key, true);
      if (!nav || !order.length) return;
      e.preventDefault();
      const alongLane = navOf(e.key) == null;
      setSelected((s) => moveIn(alongLane ? laneSteps(order, lanes, s) : order, s, nav));
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [order, lanes, setSelected]);
}
