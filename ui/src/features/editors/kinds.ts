// What each kind of item is called and where its pages live.
import type { ItemKind } from '@/model/project';

export interface KindInfo {
  one: string;
  many: string;
  route: string;
  /** What the kind is for: the list page's title hint. */
  about: string;
  /** A name such an item might have: the placeholder of its Name box. */
  example: string;
}

export const KINDS: Record<ItemKind, KindInfo> = {
  worlds: { one: 'world', many: 'Worlds', route: '/worlds', about: 'Your production systems before any agent: connectors, software flows and the data every run starts from.', example: 'Larkspur Valve Works' },
  scenarios: { one: 'scenario', many: 'Scenarios', route: '/scenarios', about: 'Incidents and tests inside a world: the ticket, what goes wrong and when, the rules and the checks.', example: 'Supplier slip during a rush order' },
  setups: { one: 'agent setup', many: 'Agent setups', route: '/setups', about: 'The agent teams you compare, built any way.', example: 'Planner and buyer' },
  experiments: { one: 'experiment', many: 'Experiments', route: '/experiments', about: 'One scenario against the setups you compare.', example: 'Supplier slip: solo vs team' },
};
