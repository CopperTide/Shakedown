import { useCallback } from 'react';
import { tasks as fetchTasks } from '@/api/benchmark';
import { taskName } from '@/model/summary';
import { useQuery } from './useQuery';

/** The benchmark's tasks, with their tickets: read again by each page that shows them. */
export function useTasks() {
  const { data: tasks, error } = useQuery('tasks', fetchTasks, { shared: true, fresh: true });
  return { tasks, error };
}

/** A task id in plain words (its one-line reason); the id itself while the tasks load or once the benchmark no longer lists it. */
export function useTaskNames(): (id: string) => string {
  const { tasks } = useTasks();
  return useCallback((id: string) => { const t = tasks?.find((x) => x.id === id); return t ? taskName(t) : id; }, [tasks]);
}
