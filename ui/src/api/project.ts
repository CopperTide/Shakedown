// The project folder: its status and its worlds, scenarios, setups and experiments (plan 1).
import type { Experiment } from '@/model/experiment';
import type { Item, ItemKind, ItemSummary, ProjectStatus, Proposed } from '@/model/project';
import { del, get, path, post, request } from './client';

export const projectStatus = () => get<ProjectStatus>('/api/project');

export const createProject = (name: string, git: boolean) =>
  request<ProjectStatus>('/api/project', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ name, git }) });

export const listItems = (kind: ItemKind) => get<{ items: ItemSummary[] }>(`/api/project/${kind}`);

export const getItem = <B = Record<string, unknown>>(kind: ItemKind, id: string) =>
  get<Item<B>>(path`/api/project/${kind}/${id}`);

/** Writes an item. With `confirm` false the answer may be a proposal (a diff) and nothing is written. */
export const putItem = <B>(kind: ItemKind, id: string, body: B, confirm = true) =>
  request<Item<B> | Proposed>(path`/api/project/${kind}/${id}`, {
    method: 'PUT',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ body, confirm }),
  });

export const deleteItem = (kind: ItemKind, id: string) => del<{ deleted: string }>(path`/api/project/${kind}/${id}`);

/** Deploys a file experiment: the engine experiment it started. */
export const deployExperiment = (id: string) => post<Experiment>(path`/api/project/experiments/${id}/deploy`, {});

/** A file experiment's deployments, newest first. */
export const experimentRuns = (id: string) => get<{ deployments: Experiment[] }>(path`/api/project/experiments/${id}/runs`);
