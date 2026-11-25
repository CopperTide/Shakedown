// The tasks of a world's package as the scenario's task picker lists them: searched, then grouped.
import type { TaskSummary } from '@/model/types';

/** The tasks of one group (null: those with none), in the package's order. */
export interface TaskGroup {
  group: string | null;
  tasks: TaskSummary[];
}

/** The tasks whose id or reason holds `query` (any case), grouped by `group` in first-seen order. */
export function taskGroups(tasks: TaskSummary[], query: string): TaskGroup[] {
  const q = query.trim().toLowerCase();
  const groups = new Map<string | null, TaskSummary[]>();
  for (const t of tasks) {
    if (q && !`${t.id}\n${t.reason}`.toLowerCase().includes(q)) continue;
    const g = t.group ?? null;
    groups.set(g, [...(groups.get(g) ?? []), t]);
  }
  return [...groups].map(([group, list]) => ({ group, tasks: list }));
}
