// Whether a file experiment has run: results, running, deployed with nothing finished, or never
// deployed. Read from its deployments (`GET /api/project/experiments/{id}/runs`). Pure.
import { MARK, type Mark, mark } from '@/lib/mark';
import type { Experiment } from '@/model/experiment';

export type RunStatus = 'results' | 'running' | 'no_results' | 'not_run';

export interface RunStatusInfo {
  status: RunStatus;
  /** Runs that ended, over every deployment. */
  ended: number;
  deployments: number;
  /** When the newest deployment started (ISO), or null when there is none. */
  last: string | null;
}

export const NOT_RUN: RunStatusInfo = { status: 'not_run', ended: 0, deployments: 0, last: null };

const endedIn = (d: Experiment) => d.setups.reduce((n, s) => n + s.summary.ended, 0);

/** The status of an experiment from its deployments (newest first, as the API lists them). */
export function runStatusOf(deployments: Experiment[]): RunStatusInfo {
  if (!deployments.length) return NOT_RUN;
  const ended = deployments.reduce((n, d) => n + endedIn(d), 0);
  const running = deployments.some((d) => d.status === 'running');
  const status: RunStatus = running ? 'running' : ended > 0 ? 'results' : 'no_results';
  const last = deployments.map((d) => d.created_at).sort().at(-1) ?? null;
  return { status, ended, deployments: deployments.length, last };
}

export const RUN_STATUS_MARK: Record<RunStatus, Mark> = {
  results: mark('■', 'Has results'),
  running: MARK.running,
  no_results: mark('□', 'Ran, no results', true),
  not_run: mark('○', 'Not run yet'),
};

/** The filter's choices, in the order the status sort puts them. */
export const RUN_STATUSES: RunStatus[] = ['results', 'running', 'no_results', 'not_run'];

/** "4 runs ended over 2 deployments · last Nov 25": the hover line of a status. */
export function runStatusTitle(i: RunStatusInfo): string {
  if (i.status === 'not_run') return 'Set up, never deployed';
  const when = i.last ? ` · last ${new Date(i.last).toLocaleDateString()}` : '';
  return `${i.ended} run${i.ended === 1 ? '' : 's'} ended over ${i.deployments} deployment${i.deployments === 1 ? '' : 's'}${when}`;
}
