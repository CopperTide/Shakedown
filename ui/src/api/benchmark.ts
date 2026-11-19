import type { BenchmarkDescription, Task, Team } from '@/model/types';
import { get } from './client';

/** What the scenario builder and the world view need of the benchmark. */
export const benchmark = () => get<BenchmarkDescription>('/api/benchmark');

export const tasks = () => get<Task[]>('/api/tasks');
export const defaultTeam = () => get<Team>('/api/team/default');
/** Every team the benchmark suggests, the default first, each with the runner it is written for. */
export const suggestedTeams = () => get<Team[]>('/api/teams');
