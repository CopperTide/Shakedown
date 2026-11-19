import { useEffect, useState } from 'react';
import { health } from '@/api/health';
import type { Health } from '@/model/types';

/** Pages dispatch this after they change how much disk the arena uses (e.g. delete a run). */
export const DISK_CHANGED = 'arena:disk-changed';

export function useHealth(): Health | null {
  const [h, setH] = useState<Health | null>(null);
  useEffect(() => {
    let alive = true;
    const load = () => health().then((v) => alive && setH(v)).catch(() => {});
    load();
    window.addEventListener(DISK_CHANGED, load);
    return () => {
      alive = false;
      window.removeEventListener(DISK_CHANGED, load);
    };
  }, []);
  return h;
}
