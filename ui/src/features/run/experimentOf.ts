// The experiment a run belongs to. No endpoint maps a run to its experiment, so it is found from
// what the API lists: the experiments that ran the run's task and started before it, newest
// first, read one at a time until one lists the run.
import { experiment, experiments } from '@/api/experiments';
import { type Column, columnOf } from '@/features/experiments/model';
import type { Experiment, ListedExperiment } from '@/model/experiment';

/** At most this many experiments are read in full while looking. */
const LOOK_AT = 5;

/** The experiments that could hold a run of `taskId` that started at `before`, newest first. */
export function candidates(list: ListedExperiment[], taskId: string | null, before: string): ListedExperiment[] {
  const t = Date.parse(before);
  return list
    .filter((e) => !e.legacy && (!taskId || e.tasks.includes(taskId)) && !(Date.parse(e.created_at) > t))
    .sort((a, b) => Date.parse(b.created_at) - Date.parse(a.created_at))
    .slice(0, LOOK_AT);
}

export const listsRun = (e: Experiment, runId: string) => e.setups.some((s) => s.runs.some((r) => r.run_id === runId));

/** The experiment that lists run `runId`, or null when none of the likely ones does. */
export async function experimentOfRun(runId: string, taskId: string | null, before: string): Promise<Experiment | null> {
  for (const c of candidates(await experiments(), taskId, before)) {
    const e = await experiment(c.id);
    if (listsRun(e, runId)) return e;
  }
  return null;
}

/**
 * The experiment's setups as strips, and its started runs in order (each with its setup and its
 * place there: run `n` of `of`) with where `runId` sits among them.
 */
export function switchOf(e: Experiment, runId: string) {
  const columns: Column[] = e.setups.map(columnOf);
  const started = columns.flatMap((c) => c.runs.flatMap((r, i) => (r.href ? [{ ...r, setup: c.name, n: i + 1, of: c.runs.length }] : [])));
  return { columns, started, at: started.findIndex((r) => r.runId === runId) };
}
