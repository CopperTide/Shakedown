import { useEffect, useState } from 'react';
import { worldViewScreen, worldViews } from '@/api/worldViews';
import { useQuery } from '@/hooks/useQuery';
import type { ScreenData } from './types';

/** The branch's world views; settled once it answered or failed (a failure means: draw the twin). */
export function useWorldViews(branchId: string) {
  const q = useQuery(branchId ? `world-views/${branchId}` : null, () => worldViews(branchId));
  return { defs: q.data, error: q.error, settled: q.data != null || q.error != null };
}

/** `value`, once it has held still for `ms` (at once when `ms` is 0). */
export function useDebounced<T>(value: T, ms: number): T {
  const [held, setHeld] = useState(value);
  useEffect(() => {
    if (ms <= 0) return;
    const id = window.setTimeout(() => setHeld(value), ms);
    return () => window.clearTimeout(id);
  }, [value, ms]);
  return ms <= 0 ? value : held;
}

export interface Stamped {
  data: ScreenData;
  /** `performance.now()` when it arrived: where interpolation of the world clock starts. */
  fetchedAt: number;
}

const MAX_CACHED = 32;
const recorded = new Map<string, Promise<Stamped>>();

/** A recorded point's screen, from memory when it was read before (a recorded step never changes). */
function cachedScreen(key: string, load: () => Promise<Stamped>): Promise<Stamped> {
  const hit = recorded.get(key);
  if (hit) { recorded.delete(key); recorded.set(key, hit); return hit; }
  const p = load();
  recorded.set(key, p);
  p.catch(() => recorded.delete(key));
  while (recorded.size > MAX_CACHED) recorded.delete(recorded.keys().next().value as string);
  return p;
}

const LIVE_MS = 1000;

/**
 * One screen at a point: polled about once a second while `live` (and again when new events come),
 * else at `at`, debounced while it moves; the last screen stays on screen while the next loads.
 */
export function useScreen(branchId: string, view: string, screen: string | null, at: string | null, live: boolean, events: number) {
  const point = live ? 'live' : at;
  const settled = useDebounced(point, live ? 0 : 150);
  const tick = useDebounced(live ? events : 0, 300);
  const key = screen ? `${branchId}|${view}|${screen}|${settled ?? ''}|${tick}` : null;
  const q = useQuery<Stamped>(key, () => loadScreen(branchId, view, screen ?? '', settled, live ? null : key), live ? POLL : ONCE);
  const fresh = isFor(q.data, view, screen);
  return { data: fresh?.data ?? null, fetchedAt: fresh?.fetchedAt ?? 0, error: q.error, pending: q.loading || settled !== point };
}

const ONCE = { keepPrevious: true, keepOnError: true };
const POLL = { ...ONCE, poll: { ms: LIVE_MS, while: () => true } };

/** The answer, when it is for the screen asked for (not the screen before, kept while the next loads). */
const isFor = (s: Stamped | null, view: string, screen: string | null) => (s && s.data.view === view && s.data.screen === screen ? s : null);

/** A screen stamped with when it came; a recorded one (`cacheKey`) from memory when read before. */
function loadScreen(branchId: string, view: string, screen: string, at: string | null, cacheKey: string | null): Promise<Stamped> {
  const load = async (): Promise<Stamped> => ({ data: await worldViewScreen(branchId, view, screen, at), fetchedAt: performance.now() });
  return cacheKey ? cachedScreen(cacheKey, load) : load();
}
