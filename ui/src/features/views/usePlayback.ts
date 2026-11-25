// Play for a recorded branch's world view: a virtual world clock runs from the stop on screen at the chosen rate;
// each stop's screen is fetched ahead and swapped in once the clock reaches it (see playback.ts). Pausing keeps the
// virtual time on screen; stepping or scrubbing goes back to the stop's own time.
import { useCallback, useEffect, useRef, useState } from 'react';
import { type Anchor, RATES, playAt, playClock, prefetchTargets, stopAtClock, stopToShow } from './playback';
import type { Stop } from './points';
import type { At } from './types';
import { hasScreen, prefetchScreen } from './useViews';

const TICK_MS = 100;
const MAX_IN_FLIGHT = 2;
/** The first guess of how long a stop takes to fetch (real ms); measured from then on. */
const FIRST_LEAD_MS = 600;

export interface PlaybackOf {
  branchId: string;
  view: string;
  screen: string | null;
  stops: Stop[];
  /** The index of the stop on screen (-1 for none). */
  shown: number;
  setStep: (seq: number) => void;
}

/** Fetches stops ahead: at most two at once, none twice at once; `lead` is how long one takes (real ms), as measured. */
function useFetcher(p: PlaybackOf) {
  const asked = useRef(new Set<string>()); // in flight
  const inFlight = useRef(0);
  const lead = useRef(FIRST_LEAD_MS);
  const { branchId, view, screen } = p;
  const fetch = useCallback((at: string) => {
    if (!screen || inFlight.current >= MAX_IN_FLIGHT || asked.current.has(at) || hasScreen(branchId, view, screen, at)) return;
    asked.current.add(at);
    inFlight.current += 1;
    const t0 = performance.now();
    const done = () => {
      inFlight.current -= 1;
      asked.current.delete(at);
      lead.current = 0.7 * lead.current + 0.3 * (performance.now() - t0);
    };
    prefetchScreen(branchId, view, screen, at).then(done, done);
  }, [branchId, view, screen]);
  return { fetch, lead };
}

/** While playing: every tick, swap in the newest stop the clock reached that is in memory, fetch ahead, stop at the end. */
function useTicker(p: PlaybackOf, playing: boolean, anchor: Anchor | null, rate: number, stop: () => void) {
  const latest = useRef(p);
  latest.current = p;
  const { fetch: fetchAhead, lead } = useFetcher(p);
  useEffect(() => {
    if (!playing || !anchor) return;
    const tick = () => {
      const { stops, shown, branchId, view, screen, setStep } = latest.current;
      if (!screen || !stops.length) return;
      const end = stops[stops.length - 1].clockMs ?? 0;
      const v = playClock(anchor, rate, performance.now(), end);
      const next = stopToShow(shown, stopAtClock(stops, v), (i) => hasScreen(branchId, view, screen, stops[i].at));
      if (next !== shown) setStep(stops[next].seq);
      for (const i of prefetchTargets(stops, v, rate, lead.current, next)) fetchAhead(stops[i].at);
      if (v >= end && next === stops.length - 1) stop();
    };
    tick();
    const id = window.setInterval(tick, TICK_MS);
    return () => window.clearInterval(id);
  }, [playing, anchor, rate, stop, fetchAhead, lead]);
}

/** Play, pause, rate and the `at` to draw by, for the recorded branch `p.stops` are of. */
export function usePlayback(p: PlaybackOf) {
  const [playing, setPlaying] = useState(false);
  const [rate, setRateState] = useState(RATES[0].worldMsPerMs);
  const [anchor, setAnchor] = useState<Anchor | null>(null);
  const endMs = p.stops.at(-1)?.clockMs ?? 0;
  const now = useCallback((a: Anchor, r: number) => playClock(a, r, performance.now(), endMs), [endMs]);
  /** At the end: stop, the clock at the end. */
  const stop = useCallback(() => { setAnchor({ clockMs: endMs, perf: performance.now() }); setPlaying(false); }, [endMs]);
  useTicker(p, playing, anchor, rate, stop);
  const { stops, shown, setStep } = p;

  /** Play from where the clock is (the stop on screen; the first when at the end); pause where the clock is. */
  const toggle = useCallback(() => {
    if (!stops.length) return;
    if (playing) { if (anchor) setAnchor({ clockMs: now(anchor, rate), perf: performance.now() }); setPlaying(false); return; }
    const atEnd = shown >= stops.length - 1 || (anchor != null && anchor.clockMs >= endMs);
    if (atEnd && stops.length > 1) setStep(stops[0].seq);
    const from = atEnd ? stops[0].clockMs : anchor?.clockMs ?? stops[Math.max(0, shown)].clockMs;
    setAnchor({ clockMs: from ?? 0, perf: performance.now() });
    setPlaying(true);
  }, [stops, shown, setStep, playing, anchor, rate, endMs, now]);

  /** Stop playing and show the stop's own time (a step, a scrub). */
  const leave = useCallback(() => { setPlaying(false); setAnchor(null); }, []);

  /** Another rate, kept from where the clock is now. */
  const setRate = useCallback((r: number) => {
    if (anchor) setAnchor({ clockMs: playing ? now(anchor, rate) : anchor.clockMs, perf: performance.now() });
    setRateState(r);
  }, [anchor, playing, rate, now]);

  /** The point's `at` as the screen draws it: by the virtual clock while playing or paused in play, else as it came. */
  const shownAt = useCallback((at: At | null, fetchedAt: number) =>
    (at && anchor ? { at: playAt(at, anchor, playing ? rate : 0), fetchedAt: anchor.perf } : { at, fetchedAt }), [anchor, playing, rate]);

  return { playing, rate, setRate, toggle, leave, shownAt };
}
