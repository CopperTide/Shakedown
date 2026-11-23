// Worlds under their groups, and the scenarios and experiments under each world: sorted, pure.
import type { ItemSummary, WorldBody } from '@/model/project';
import { setIn } from './edit';

/** One group's worlds; `name` is null for the worlds in no group. */
export interface WorldGroup {
  name: string | null;
  worlds: ItemSummary[];
}

const byText = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: 'base' });

/** Items by label, then by id. */
export const byLabel = (a: ItemSummary, b: ItemSummary) => byText(a.label, b.label) || byText(a.id, b.id);

/** The groups the worlds are in, each once, A to Z. */
export const groupNames = (worlds: ItemSummary[]): string[] =>
  [...new Set(worlds.flatMap((w) => (w.group ? [w.group] : [])))].sort(byText);

/** The worlds under their groups: groups A to Z, the worlds in no group last, worlds by label. */
export function groupWorlds(worlds: ItemSummary[]): WorldGroup[] {
  const sorted = [...worlds].sort(byLabel);
  const groups: WorldGroup[] = groupNames(worlds).map((name) => ({ name, worlds: sorted.filter((w) => w.group === name) }));
  const ungrouped = sorted.filter((w) => !w.group);
  return ungrouped.length ? [...groups, { name: null, worlds: ungrouped }] : groups;
}

/** The scenarios that happen in `world`, by label. */
export const scenariosIn = (scenarios: ItemSummary[], world: string) => scenarios.filter((s) => s.world === world).sort(byLabel);

/** The experiments that play `scenario`, by label. */
export const experimentsUsing = (experiments: ItemSummary[], scenario: string) => experiments.filter((e) => e.scenario === scenario).sort(byLabel);

/** `body` in `group`; null takes it out of any group. */
export const withGroup = (body: WorldBody, group: string | null): WorldBody => setIn(body, ['group'], group ?? undefined);
