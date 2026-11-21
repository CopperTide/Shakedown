// The Get started steps and whether each is done, from live state only. Pure.
import type { ItemKind } from '@/model/project';

/** Where the project stands: open, not created yet, or shaked started without a folder. */
export type ProjectStage = 'open' | 'create' | 'not_configured';

export interface StepState { id: 'project' | 'model' | 'world' | 'agents' | 'scenario' | 'ready'; title: string; done: boolean }

/** The stage the `/api/project` answer means; 409 messages tell "no folder" from "not created". */
export function stageOf(open: boolean, message?: string): ProjectStage {
  if (open) return 'open';
  return message?.includes('SHAKEDOWN_PROJECT') ? 'not_configured' : 'create';
}

export function stepStates({ project, counts, providersOk }: { project: ProjectStage; counts: Partial<Record<ItemKind, number>>; providersOk: boolean }): StepState[] {
  const has = (k: ItemKind) => (counts[k] ?? 0) > 0;
  const steps: StepState[] = [
    { id: 'project', title: 'Your project', done: project === 'open' },
    { id: 'model', title: 'Connect a model', done: providersOk },
    { id: 'world', title: 'Your world', done: has('worlds') },
    { id: 'agents', title: 'Your agents', done: has('setups') },
    { id: 'scenario', title: 'First scenario', done: has('scenarios') },
  ];
  return [...steps, { id: 'ready', title: 'Ready', done: steps.every((s) => s.done) }];
}
