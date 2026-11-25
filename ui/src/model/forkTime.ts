// A fork on its own clock. A fork's record holds its parent's steps up to the fork point
// (recorded when the parent ran) and its own steps (recorded when the fork ran, possibly days
// later). Drawn on wall time, that gap would squeeze both parts to the edges of the timeline, so
// the fork's own steps are moved back to start just after the fork point. Spacing within each
// part is kept; only the display moves, never the record.
import type { Event } from '@/model/types';

/** How far after the fork point the fork's first own step is drawn. */
export const FORK_GAP_MS = 1000;

/** How many ms the fork's own steps (seq >= `startSeq`) move back; 0 when there is nothing to close. */
export function forkShiftMs(events: Event[], startSeq: number | null): number {
  if (startSeq == null) return 0;
  let lastInherited = -Infinity;
  let firstOwn = Infinity;
  for (const e of events) {
    const at = Date.parse(e.at);
    if (Number.isNaN(at)) continue;
    if (e.seq < startSeq) lastInherited = Math.max(lastInherited, at);
    else firstOwn = Math.min(firstOwn, at);
  }
  if (!Number.isFinite(lastInherited) || !Number.isFinite(firstOwn)) return 0;
  return Math.max(0, firstOwn - lastInherited - FORK_GAP_MS);
}

/** `events` with the fork's own steps on the fork's clock (the same array when nothing moves). */
export function onForkClock(events: Event[], startSeq: number | null): Event[] {
  const shift = forkShiftMs(events, startSeq);
  if (shift === 0 || startSeq == null) return events;
  return events.map((e) => (e.seq < startSeq ? e : { ...e, at: new Date(Date.parse(e.at) - shift).toISOString() }));
}
