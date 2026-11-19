import type { ScenarioChoice } from '@/model/scenario';
import type { NewRunResponse, RunSummary, Runner, Team } from '@/model/types';
import { del, get, path, post } from './client';

export const runs = () => get<RunSummary[]>('/api/runs');

/** Starts a run; with `connection`, Shakedown launches that Code connection's driver, which drives it. */
export const startRun = (task_id: string, team: Team, runner: Runner = 'chain', scenario?: ScenarioChoice, connection?: string) =>
  post<NewRunResponse>('/api/runs', {
    task_id, team, runner: connection ? 'external' : runner, ...(scenario ? { scenario } : {}), ...(connection ? { connection } : {}),
  });

export const deleteRun = (id: string) => del<{ dropped: number }>(path`/api/runs/${id}`);
