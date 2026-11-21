// What an experiment can choose from: the project's worlds, its scenarios (with the world each
// happens in) and its setups.
import { getItem, listItems } from '@/api/project';
import type { ItemSummary } from '@/model/project';

export interface ScenarioChoice { id: string; label: string; world: string }

export interface Choices {
  worlds: ItemSummary[];
  setups: ItemSummary[];
  scenarios: ScenarioChoice[];
}

export async function loadChoices(): Promise<Choices> {
  const [worlds, setups, scenarios] = await Promise.all([listItems('worlds'), listItems('setups'), listItems('scenarios')]);
  const withWorld = await Promise.all(scenarios.items.map(async (s) => {
    const { body } = await getItem<{ world: string }>('scenarios', s.id);
    return { id: s.id, label: s.label, world: body.world };
  }));
  return { worlds: worlds.items, setups: setups.items, scenarios: withWorld };
}

/** The scenarios that happen in `world`. */
export const scenariosOf = (scenarios: ScenarioChoice[], world: string) => scenarios.filter((s) => s.world === world);
