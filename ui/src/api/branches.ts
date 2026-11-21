import type { BranchView, Recording } from '@/model/twin';
import type {
  BranchDetail, Change, Event, ForkResponse, RestoreMode, RestorePlan, RowsResponse, StatusEvent, Team,
} from '@/model/types';
import { get, path, post, qs } from './client';
import { api } from './core';

export const branch = (id: string) => get<BranchDetail>(path`/api/branches/${id}`);

export const rows = (id: string, seq: number, keys: string[]) =>
  get<RowsResponse>(path`/api/branches/${id}/rows` + qs({ seq, keys: keys.join(',') }));

/** Forks at `at_seq` with agent `changes`, or with another `team` (a run an external system drives only). */
export const fork = (id: string, at_seq: number, changes: Record<string, Change> = {}, restore: RestoreMode = 'exact', team?: Team) =>
  post<ForkResponse>(path`/api/branches/${id}/fork`, team ? { at_seq, restore, team } : { at_seq, changes, restore });

/** How each part of a fork at `at_seq` would be restored; a 409 names the part that can't be. */
export const restorePlan = (id: string, at_seq: number, restore: RestoreMode) =>
  get<RestorePlan>(path`/api/branches/${id}/restore-plan` + qs({ at_seq, restore }));

/** Pause or resume a live branch: its calls wait while paused (`{paused, changed}`). */
export const setPaused = (id: string, paused: boolean) =>
  post<{ paused: boolean; changed: boolean }>(path`/api/branches/${id}/${paused ? 'pause' : 'resume'}`, {});

/** Stop a live branch: it ends `stopped`, unscored; the stream's status says when. */
export const stopBranch = (id: string) => post<{ stopping: boolean }>(path`/api/branches/${id}/stop`, {});

/** The branch's digital twin after each step that wrote anything, inherited views included. */
export const views = (id: string) => get<BranchView[]>(path`/api/branches/${id}/views`);

/** Whether the branch (or the parent it inherits from) has a plant recording. */
export const recording = (id: string) => get<Recording>(path`/api/branches/${id}/recording`);

/** Subscribe to a branch's SSE feed. Returns a function that closes it. */
export function stream(
  id: string,
  onEvent: (e: Event) => void,
  onStatus: (s: StatusEvent) => void,
): () => void {
  const es = new EventSource(api(path`/api/branches/${id}/stream`));
  const listen = <T>(name: string, fn: (v: T) => void) =>
    es.addEventListener(name, (m) => {
      try {
        fn(JSON.parse((m as MessageEvent).data) as T);
      } catch {
        /* malformed frame: skip */
      }
    });
  listen<Event>('event', onEvent);
  listen<StatusEvent>('status', onStatus);
  return () => es.close();
}
