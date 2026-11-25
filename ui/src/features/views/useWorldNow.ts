import { useEffect, useState } from 'react';
import { worldNow } from './geo';
import type { At } from './types';

/**
 * The world time now (epoch ms): `at.world_ms`, run on at its speed since `fetchedAt` and read again every `everyMs`
 * while the clock runs (a live branch, or a recorded one playing); `at.world_ms` while it stands still.
 */
export function useWorldNow(at: At | null, fetchedAt: number, everyMs = 250): number | null {
  const [, setTick] = useState(0);
  const speed = at && !at.paused ? at.speed : 0;
  useEffect(() => {
    if (!(speed > 0)) return;
    const id = window.setInterval(() => setTick((t) => t + 1), everyMs);
    return () => window.clearInterval(id);
  }, [speed, everyMs]);
  return worldNow(at?.world_ms ?? null, speed, fetchedAt, performance.now());
}
