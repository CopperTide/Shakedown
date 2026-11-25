import { useCallback, useEffect } from 'react';
import { branch as fetchBranch, stream } from '@/api/branches';
import { runs as fetchRuns } from '@/api/runs';
import { useQuery } from '@/hooks/useQuery';
import type { Branch, BranchDetail, Event } from '@/model/types';

/** Adds a streamed event to the branch on screen, once. */
export function withEvent(d: BranchDetail | null, id: string, e: Event): BranchDetail | null {
  if (!d || d.id !== id || d.events.some((x) => x.seq === e.seq)) return d;
  return { ...d, events: [...d.events, e] };
}

/**
 * A branch with its events, live while it runs; with `runId`, its run's other branches too. `lean` reads model calls
 * without their request and response (most of a long run's record), for pages that show the world, not the calls.
 */
export function useBranch(branchId: string, runId?: string, { lean = false }: { lean?: boolean } = {}) {
  // A failed read keeps what is on screen; a response lands only if it is still for the branch on screen.
  const branch = useQuery(lean ? `${branchId}?lean` : branchId, () => fetchBranch(branchId, lean), { keepOnError: true });
  const others = useQuery(runId ? `${runId}/${branchId}` : null, async () => (await fetchRuns()).find((x) => x.id === runId)?.branches ?? [],
    { keepOnError: true, keepPrevious: true });
  const { reload: reloadBranch, update } = branch;
  const { reload: reloadOthers } = others;
  const reload = useCallback(() => { reloadBranch(); reloadOthers(); }, [reloadBranch, reloadOthers]);
  const running = branch.data?.status === 'running';
  useEffect(() => {
    if (!running) return;
    return stream(branchId, (e) => update((d) => withEvent(d, branchId, e)), reload);
  }, [running, branchId, update, reload]);
  return { detail: branch.data, siblings: others.data ?? NO_BRANCHES, error: branch.error, reload };
}

const NO_BRANCHES: Branch[] = [];
