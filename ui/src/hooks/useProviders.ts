import { useEffect, useSyncExternalStore } from 'react';
import { providers as fetchProviders } from '@/api/health';
import { useHealth } from '@/hooks/useHealth';
import { errorMessage } from '@/lib/errors';
import type { ProviderStatus } from '@/model/types';

/** The last `/api/providers` answer, shared by every page (and the status line). */
let checked: ProviderStatus[] | null = null;
let pending: Promise<ProviderStatus[]> | null = null;
let failure: string | null = null;
const listeners = new Set<() => void>();
const notify = () => listeners.forEach((f) => f());

interface Snapshot { list: ProviderStatus[] | null; loading: boolean; error: string | null }
let snapshot: Snapshot = { list: null, loading: false, error: null };
const refreshSnapshot = () => { snapshot = { list: checked, loading: pending != null, error: failure }; notify(); };

/** Asks `/api/providers` (once at a time) and shares the answer. */
export function checkProviders(): Promise<ProviderStatus[]> {
  if (pending) return pending;
  pending = fetchProviders()
    .then((v) => { checked = v; failure = null; return v; })
    .catch((e) => { failure = errorMessage(e); throw e; })
    .finally(() => { pending = null; refreshSnapshot(); });
  refreshSnapshot();
  return pending;
}

/** For tests: forget the shared answer. */
export function resetProviders(list: ProviderStatus[] | null = null): void {
  checked = list; pending = null; failure = null; refreshSnapshot();
}

function subscribe(f: () => void) {
  listeners.add(f);
  return () => { listeners.delete(f); };
}

/** The checked provider list (`check`: ask for it on mount unless it's already known). */
export function useCheckedProviders(check = false): Snapshot {
  const s = useSyncExternalStore(subscribe, () => snapshot, () => snapshot);
  useEffect(() => {
    if (check && !checked) checkProviders().catch(() => {});
  }, [check]);
  return s;
}

/** The checked list once there is one, else the health summary (locals not checked). */
export const bestProviders = (checkedList: ProviderStatus[] | null, health: ProviderStatus[] | undefined) =>
  checkedList ?? health ?? [];

/** The providers to pick from: asks `/api/providers` on mount, the health summary until it answers. */
export function useProviderList(): { list: ProviderStatus[]; loading: boolean; error: string | null } {
  const health = useHealth();
  const s = useCheckedProviders(true);
  return { list: bestProviders(s.list, health?.providers), loading: s.loading, error: s.error };
}
