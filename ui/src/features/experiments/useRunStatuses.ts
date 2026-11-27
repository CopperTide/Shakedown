// Whether each file experiment ran, read from its deployments; read again while one runs.
import { experimentRuns } from '@/api/project';
import { pollWhile, useQuery } from '@/hooks/useQuery';
import { type RunStatusInfo, runStatusOf } from './runStatus';

const POLL_MS = 5000;

type Statuses = Record<string, RunStatusInfo>;

/** Each experiment's status; one that can't be read is left out (shown as unknown). */
async function load(ids: string[]): Promise<Statuses> {
  const got = await Promise.all(ids.map((id) => experimentRuns(id).then((r) => [id, runStatusOf(r.deployments)] as const, () => null)));
  return Object.fromEntries(got.filter((g) => g !== null));
}

const anyRunning = (s: Statuses) => Object.values(s).some((i) => i.status === 'running');

/** The status of each of `ids` by id; empty until read. */
export function useRunStatuses(ids: string[]): Statuses {
  const key = ids.length ? `experiment-status:${[...ids].sort().join(',')}` : null;
  return useQuery(key, () => load(ids), pollWhile(POLL_MS, anyRunning)).data ?? {};
}
