import type { Experiment, ListedExperiment, NewExperiment } from '@/model/experiment';
import { tracked } from '@/lib/productAnalytics';
import { get, path, post } from './client';

export const experiments = () => get<ListedExperiment[]>('/api/experiments');
export const experiment = (id: string) => get<Experiment>(path`/api/experiments/${id}`);
export const createExperiment = (e: NewExperiment) => post<Experiment>('/api/experiments', e).then(tracked('experiment run started'));
export const stopExperiment = (id: string) => post<unknown>(path`/api/experiments/${id}/stop`, {});
