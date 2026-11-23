import type { ScenarioChoice } from '@/model/scenario';
import type { NewRunResponse, RunSummary, Runner, Team } from '@/model/types';
import { tracked } from '@/lib/productAnalytics';
import { del, get, path, post } from './client';

export const runs = () => get<RunSummary[]>('/api/runs');

/** How a run starts: its runner, scenario and Code connection, and the benchmark it runs on (the page's; absent, the current one). */
export interface RunChoice {
  runner?: Runner;
  scenario?: ScenarioChoice;
  connection?: string;
  benchmark?: string;
}

/** Starts a run; with `connection`, Shakedown launches that Code connection's driver, which drives it. */
export const startRun = (task_id: string, team: Team, { runner = 'chain', scenario, connection, benchmark }: RunChoice = {}) =>
  post<NewRunResponse>('/api/runs', {
    task_id, team, runner: connection ? 'external' : runner, ...(scenario ? { scenario } : {}), ...(connection ? { connection } : {}),
    ...(benchmark ? { benchmark } : {}),
  }).then(tracked('run started'));

export const deleteRun = (id: string) => del<{ dropped: number }>(path`/api/runs/${id}`);
