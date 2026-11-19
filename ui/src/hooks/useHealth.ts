import { useEffect } from 'react';
import { health } from '@/api/health';
import type { Health } from '@/model/types';
import { useQuery } from './useQuery';

/** Pages dispatch this after they change how much disk the arena uses (e.g. delete a run). */
export const DISK_CHANGED = 'arena:disk-changed';

/** arenad's health, read again by each page that shows it and after `DISK_CHANGED`; a failed read keeps the last one. */
export function useHealth(): Health | null {
  const { data, reload } = useQuery('health', health, { shared: true, fresh: true, keepOnError: true });
  useEffect(() => {
    window.addEventListener(DISK_CHANGED, reload);
    return () => window.removeEventListener(DISK_CHANGED, reload);
  }, [reload]);
  return data;
}
