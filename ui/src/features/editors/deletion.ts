// What the delete dialog says about the items that use the one being deleted. Pure.
import type { Dependent, Fate } from '@/model/project';

const count = (n: number, one: string) => `${n} ${one}${n === 1 ? '' : 's'}`;

/**
 * The button that deletes with everything that uses the item, naming what it takes:
 * "Delete with 1 scenario, 2 experiments", "Delete and update 3 experiments", or both.
 */
export function cascadeLabel(deps: Dependent[]): string {
  const deleted = deps.filter((d) => d.fate === 'deleted');
  const scenarios = deleted.filter((d) => d.kind === 'scenario').length;
  const experiments = deleted.filter((d) => d.kind === 'experiment').length;
  const gone = [scenarios && count(scenarios, 'scenario'), experiments && count(experiments, 'experiment')].filter(Boolean).join(', ');
  const changed = deps.length - deleted.length;
  const update = changed ? `update ${count(changed, 'experiment')}` : '';
  if (gone && update) return `Delete with ${gone} and ${update}`;
  return gone ? `Delete with ${gone}` : `Delete and ${update}`;
}

/** Every fate of a dependent (an older answer names only the first). */
export const fatesOf = (d: Dependent): Fate[] => d.fates ?? [d.fate];

/** What happens to a dependent that stays, as the dialog tags it. */
export const KEPT_AS: Record<Fate, string | null> = {
  deleted: null,
  setup_dropped: 'keeps its other setups',
  parent_cleared: 'becomes a top-level experiment',
};
