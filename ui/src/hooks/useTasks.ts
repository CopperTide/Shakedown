import { useCallback, useEffect, useState } from 'react';
import { tasks as fetchTasks } from '@/api';
import { taskName } from '@/home/summary';
import { errorMessage } from '@/lib/errors';
import type { Task } from '@/types';

/** The benchmark's tasks, with their tickets. */
export function useTasks() {
  const [tasks, setTasks] = useState<Task[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { fetchTasks().then(setTasks).catch((e) => setError(errorMessage(e))); }, []);
  return { tasks, error };
}

/** A task id in plain words (its one-line reason); the id itself while the tasks load or once the benchmark no longer lists it. */
export function useTaskNames(): (id: string) => string {
  const { tasks } = useTasks();
  return useCallback((id: string) => { const t = tasks?.find((x) => x.id === id); return t ? taskName(t) : id; }, [tasks]);
}
