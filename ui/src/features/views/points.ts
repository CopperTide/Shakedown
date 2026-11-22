// The points of a branch a view can be evaluated at: live while it runs; else a stop per step where the
// world clock was read (a tool call, a world event), and the check's final state at the end.
import { asRecord } from '@/lib/record';
import type { Event } from '@/model/types';
import { snapStop } from '@/features/world/scrub';

export interface Stop {
  seq: number;
  /** The `at` it is fetched with: a seq, or `final`. */
  at: string;
  clockMs: number | null;
}

const clockOf = (e: Event): number | null => {
  const v = asRecord(e.payload).clock_ms;
  return typeof v === 'number' && Number.isFinite(v) ? v : null;
};

/** The stops of a recorded branch, in step order: each event with a world clock, then `final` at its check. */
export function viewStops(events: Event[]): Stop[] {
  const out: Stop[] = [];
  let lastClock: number | null = null;
  for (const e of [...events].sort((a, b) => a.seq - b.seq)) {
    const clock = clockOf(e);
    if (e.kind === 'check') { out.push({ seq: e.seq, at: 'final', clockMs: lastClock }); continue; }
    if (clock == null) continue;
    lastClock = clock;
    out.push({ seq: e.seq, at: String(e.seq), clockMs: clock });
  }
  // One final stop, at the last check.
  const finals = out.filter((s) => s.at === 'final');
  return finals.length > 1 ? out.filter((s) => s.at !== 'final' || s === finals[finals.length - 1]) : out;
}

/** The `at` for the step in the address: the stop it snaps to, the last stop when none; null (core's default point) without stops. */
export function atFor(stops: Stop[], step: number | null): { at: string | null; seq: number | null } {
  const seq = snapStop(stops.map((s) => s.seq), step);
  const stop = stops.find((s) => s.seq === seq);
  return stop ? { at: stop.at, seq: stop.seq } : { at: null, seq: null };
}

/** Whether a branch is running (its views are read live). */
export const isLive = (status: string | undefined) => status === 'running';
