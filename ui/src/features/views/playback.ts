// Playback of a recorded branch: a virtual world clock runs at a chosen rate from the stop on screen, moving docs
// are placed by it every frame (so they glide between stops), and each stop's screen is swapped in as the clock
// reaches it, fetched ahead so playback never waits. Pure: the hook in usePlayback.ts drives it.
import type { Stop } from './points';
import type { At } from './types';

/** The rates playback runs at: 1× is an hour of world time in 10 s of real time. */
export const RATES = [1, 4, 16].map((x) => ({ label: `${x}×`, worldMsPerMs: 360 * x }));

/** Where the virtual clock was started: a branch clock (ms) at a `performance.now()`. */
export interface Anchor {
  clockMs: number;
  perf: number;
}

/** The index of the last stop whose clock is at or before `clockMs` (-1 before the first): the state then. */
export function stopAtClock(stops: Stop[], clockMs: number): number {
  let lo = 0, hi = stops.length - 1, found = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const c = stops[mid].clockMs ?? -Infinity;
    if (c <= clockMs) { found = mid; lo = mid + 1; } else hi = mid - 1;
  }
  return found;
}

/** The virtual clock at `perfNow`: `rate` world ms per real ms since the anchor, never past `endMs`. */
export const playClock = (a: Anchor, rate: number, perfNow: number, endMs: number) =>
  Math.min(endMs, a.clockMs + rate * Math.max(0, perfNow - a.perf));

/**
 * The stop's `at` as the virtual clock shows it: at the anchor's clock, running on at `rate` from the anchor's
 * `performance.now()` (pass that as `fetchedAt`), so `worldNow` and the map place moving docs by the virtual clock.
 */
export function playAt(at: At, a: Anchor, rate: number): At {
  const epoch = at.world_ms != null && at.clock_ms != null ? at.world_ms - at.clock_ms : null;
  return { ...at, clock_ms: a.clockMs, world_ms: epoch == null ? null : epoch + a.clockMs, speed: rate, paused: false };
}

/** The stop to show: the newest loaded stop after the one shown, up to the one the clock has reached (else the same). */
export function stopToShow(shown: number, due: number, loaded: (i: number) => boolean): number {
  for (let i = due; i > shown; i--) if (loaded(i)) return i;
  return shown;
}

/**
 * The stops to fetch now: the one the clock has reached, and the ones it reaches within one and two fetch times
 * (`leadMs` of real time); only those after the one shown, each once.
 */
export function prefetchTargets(stops: Stop[], clockMs: number, rate: number, leadMs: number, shown: number): number[] {
  const at = [0, 1, 2].map((k) => stopAtClock(stops, clockMs + k * rate * leadMs));
  return [...new Set(at)].filter((i) => i > shown);
}
