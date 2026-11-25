import { expect, it } from 'vitest';
import { events, team } from '@/model/fixtures';
import { buildLanes } from '@/model/lanes';
import type { Placed } from './timelineLayout';
import { groupMarks, visibleGroups } from './timelineVisibility';

it('bounds visible targets for 20,000 crowded events without losing events', () => {
  const mark = buildLanes(events, team)[0].marks[0];
  const placed: Placed[] = Array.from({ length: 20000 }, (_, seq) => ({
    mark: { ...mark, seq }, lane: seq % 4, x: seq / 10, labelled: false,
  }));
  const groups = groupMarks(placed);
  expect(groups.reduce((n, g) => n + g.marks.length, 0)).toBe(20000);
  const visible = visibleGroups(groups, 700, 500);
  expect(visible.length).toBeLessThan(120);
  expect(visible.every((g) => g.x >= 620 && g.x <= 1280)).toBe(true);
  expect(groups.every((g) => g.marks.every((p) => p.lane === g.lane))).toBe(true);
  expect(groupMarks(placed.map((p) => ({ ...p, x: p.x * 10 }))).length).toBeGreaterThan(groups.length);
});
