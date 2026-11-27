// The Get started steps and whether each is done, from live state only. Pure. Setup is the project
// and a model; worlds, agents and scenarios are made in the app itself.

/** Where the project stands: open, not created yet, or shaked started without a folder. */
export type ProjectStage = 'open' | 'create' | 'not_configured';

export interface StepState { id: 'project' | 'model'; title: string; done: boolean }

/** The stage the `/api/project` answer means; 409 messages tell "no folder" from "not created". */
export function stageOf(open: boolean, message?: string): ProjectStage {
  if (open) return 'open';
  return message?.includes('SHAKEDOWN_PROJECT') ? 'not_configured' : 'create';
}

export function stepStates({ project, providersOk }: { project: ProjectStage; providersOk: boolean }): StepState[] {
  return [
    { id: 'project', title: 'Your project', done: project === 'open' },
    { id: 'model', title: 'Connect a model', done: providersOk },
  ];
}

/** Whether setup is done: what opens the rest of Shakedown. */
export const coreDone = (steps: StepState[]) => steps.every((s) => s.done);

/** The step Get started shows unless one is picked: the first not done, else the last. */
export const firstOpen = (steps: StepState[]): StepState['id'] => (steps.find((s) => !s.done) ?? steps[steps.length - 1]).id;
