// The smallest valid file for each kind: what a new item starts from (blank by default).
import type { ItemKind } from '@/model/project';

export const TEMPLATES: Record<ItemKind, (label: string) => Record<string, unknown>> = {
  worlds: (label) => ({ format: 'shakedown-world/3', label }),
  scenarios: (label) => ({ format: 'shakedown-scenario/1', label, world: '', task: '' }),
  setups: (label) => ({ format: 'shakedown-setup/1', label, team: { kind: 'builtin', agents: [] } }),
  experiments: (label) => ({ format: 'shakedown-experiment/1', label, world: '', scenario: '', setups: [], repeats: { per_setup: 1 } }),
};
