// The plant's recording for the world view: Isaac Sim writes its frames on arena time, so a step's arena second is the
// video's second. `GET /api/branches/{id}/recording` says whether there is one (`video` null when not).
import { asRecord } from '@/lib/record';
import type { Event } from '@/model/types';

/** The arena second of step `seq`: its own world clock, else the last one recorded before it (null when none). */
export function arenaSecondAt(events: Event[], seq: number | null): number | null {
  if (seq == null) return null;
  let found: number | null = null;
  for (const e of events) {
    if (e.seq > seq) break;
    const ms = asRecord(e.payload).clock_ms;
    if (typeof ms === 'number') found = ms / 1000;
  }
  return found;
}

/** `m:ss` of arena seconds. */
export function clockText(s: number): string {
  const whole = Math.max(0, Math.floor(s));
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, '0')}`;
}
