// Worlds under their groups, and the scenarios and experiments under each: sorted, pure.
import { describe, expect, it } from 'vitest';
import type { ItemSummary } from '@/model/project';
import { experimentsUsing, groupNames, groupWorlds, scenariosIn, withGroup } from './tree';

const item = (id: string, label: string, more: Partial<ItemSummary> = {}): ItemSummary =>
  ({ kind: 'world', id, label, hash: 'h', problems: [], ...more });

const WORLDS = [
  item('plant-b', 'West plant', { group: 'Rebolt' }),
  item('solo', 'Larkspur'),
  item('plant-a', 'East plant', { group: 'Rebolt' }),
  item('mill', 'Mill', { group: 'Arden' }),
  item('zed', 'Zed', { group: null }),
];

describe('world tree', () => {
  it('groups_sorted_with_ungrouped_last', () => {
    const groups = groupWorlds(WORLDS);
    expect(groups.map((g) => g.name)).toEqual(['Arden', 'Rebolt', null]);
    expect(groups[1].worlds.map((w) => w.label)).toEqual(['East plant', 'West plant']);
    expect(groups[2].worlds.map((w) => w.label)).toEqual(['Larkspur', 'Zed']);
  });

  it('no_ungrouped_header_when_every_world_has_a_group', () => {
    expect(groupWorlds(WORLDS.filter((w) => w.group)).map((g) => g.name)).toEqual(['Arden', 'Rebolt']);
  });

  it('group_names_once_each_sorted', () => {
    expect(groupNames(WORLDS)).toEqual(['Arden', 'Rebolt']);
  });

  it('scenarios_of_a_world_and_experiments_of_a_scenario_by_label', () => {
    const scenarios = [item('s2', 'Tank leak', { world: 'mill' }), item('s1', 'Belt jam', { world: 'mill' }), item('s3', 'Other', { world: 'solo' })];
    expect(scenariosIn(scenarios, 'mill').map((s) => s.id)).toEqual(['s1', 's2']);
    const experiments = [item('e2', 'B', { scenario: 's1' }), item('e1', 'A', { scenario: 's1' }), item('e3', 'C', { scenario: 's2' })];
    expect(experimentsUsing(experiments, 's1').map((e) => e.id)).toEqual(['e1', 'e2']);
  });

  it('with_group_sets_or_removes_the_key', () => {
    const body = { format: 'shakedown-world/3', label: 'W', group: 'Old' };
    expect(withGroup(body, 'New').group).toBe('New');
    expect('group' in withGroup(body, null)).toBe(false);
    expect(body.group).toBe('Old');
  });
});
