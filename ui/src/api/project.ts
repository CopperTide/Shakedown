// The project folder: its status and its worlds, scenarios, setups and experiments (plan 1).
import type { Experiment } from '@/model/experiment';
import type { DeletePreview, Deleted, Dependent, Item, ItemKind, ItemSummary, LastDelete, ProjectSettings, ProjectStatus, Proposed } from '@/model/project';
import { tracked } from '@/lib/productAnalytics';
import { get, path, post, request } from './client';

export const projectStatus = () => get<ProjectStatus>('/api/project');

export const createProject = (name: string, git: boolean) =>
  request<ProjectStatus>('/api/project', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ name, git }) })
    .then(tracked('project created'));

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

/**
 * Deletes an item, confirmed (the person deciding is the confirmation). Refused (409, with
 * `dependents`) while anything uses it unless `cascade`, and when what uses it is no longer `expect`.
 */
export const deleteItem = (kind: ItemKind, id: string, cascade = false, expect?: Dependent[]) =>
  request<Deleted>(path`/api/project/${kind}/${id}`, {
    method: 'DELETE',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ cascade, confirm: true, ...(expect ? { expect } : {}) }),
  });

/** What uses an item, what deleting it with everything that uses it does to each, and the folder that stays. */
export const dependentsOf = (kind: ItemKind, id: string) => get<DeletePreview>(path`/api/project/${kind}/${id}/dependents`);

/** The most recent delete that can be undone (404 when there is none). */
export const lastDelete = () => get<LastDelete>('/api/project/trash');

/** Undoes the delete whose files went to `trash` (the most recent when missing). */
export const restoreDeleted = (trash?: string) =>
  post<{ restored: string; files: string[] } | Proposed>('/api/project/trash/restore', { trash, confirm: true });

/** Renames a world group, moving every world in it; a null `to` takes them out of any group. */
export const renameGroup = (from: string, to: string | null) =>
  post<{ moved: string[] } | Proposed>('/api/project/world-groups/rename', { from, to, confirm: true });

/** Deploys a file experiment: the engine experiment it started. */
export const deployExperiment = (id: string) => post<Experiment>(path`/api/project/experiments/${id}/deploy`, {}).then(tracked('experiment deployed'));

/** A file experiment's deployments, newest first. */
export const experimentRuns = (id: string) => get<{ deployments: Experiment[] }>(path`/api/project/experiments/${id}/runs`);

/** Writes the project's settings (confirmed: the person saving is the confirmation). */
export const updateSettings = (body: ProjectSettings) =>
  request<ProjectStatus | Proposed>('/api/project', { method: 'PUT', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ body, confirm: true }) });

export interface MadlGraph {
  agents: { id: string; label: string; runtime?: string | null }[];
  edges: { from: string; to: string; kind: string }[];
}

export interface MadlReport {
  valid: boolean;
  diagnostics: { code: string; severity: string; pointer: string; message: string }[];
  graph: MadlGraph;
}

/** A MADL setup's checks and graph. */
export const madlOf = (id: string) => get<MadlReport>(path`/api/project/setups/${id}/madl`);

export interface ImportPlan { proposed?: true; files?: { kind: string; id: string; label: string }[]; written?: { kind: string; id: string; label: string }[]; linked?: number; skipped: string[] }

/** Moves earlier work into the project; without `confirm`, only what it would write. */
export const importProject = (confirm: boolean) => post<ImportPlan>('/api/project/import', { confirm });

/** What importing a git repo found, and what happens (or happened) to each piece. */
export interface GitImport {
  proposed?: true;
  id: string;
  url: string;
  ref: string;
  commit: string;
  project: { name: string; problems: string[] } | null;
  packages: { name: string; kind: string; dir: string; python: string | null }[];
  items: { kind: string; id: string; label: string; action: 'write' | 'replace' | 'same' | 'keep'; reason: string | null }[];
  views: { world: string; file: string }[];
  compose: { file: string; services: string[]; volumes: string[] }[];
  notes: string[];
  problems: string[];
  conflicts: string[];
  previous_commit: string | null;
  registered?: boolean;
}

/** Imports a git repo (a URL or a local path) at a ref; without `confirm`, only the plan. */
export const importGit = (url: string, ref: string, confirm: boolean) =>
  post<GitImport>('/api/project/import-git', { url, ref: ref || undefined, confirm });
/** The files in a world's data folder, relative to it. */
export const worldDataFiles = (id: string) =>
  get<{ files: { path: string; size: number }[]; truncated: boolean }>(path`/api/project/worlds/${id}/data`);

/** One data file: its text (cut at 1 MiB, then `truncated`), or `binary` with its size. */
export const worldDataFile = (id: string, file: string) =>
  get<{ path: string; size: number; text?: string; truncated?: boolean; binary?: boolean }>(path`/api/project/worlds/${id}/data?path=${file}`);
