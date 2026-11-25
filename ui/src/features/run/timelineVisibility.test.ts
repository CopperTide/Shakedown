import { expect, it } from 'vitest';
import { events, team } from '@/model/fixtures';
import { buildLanes } from '@/model/lanes';
import type { Placed } from './timelineLayout';
import { groupMarks, marksToDraw, visibleGroups } from './timelineVisibility';

it('bounds visible targets for 20,000 crowded events without losing events', () => {
  const mark = buildLanes(events, team)[0].marks[0];
  const placed: Placed[] = Array.from({ length: 20000 }, (_, seq) => ({
    mark: { ...mark, seq }, lane: seq % 4, x: Math.floor(seq / 4) / 10, labelled: false,
  }));
  const groups = groupMarks(placed);
  expect(groups.reduce((n, g) => n + g.marks.length, 0)).toBe(20000);
  const visible = visibleGroups(groups, 100, 200);
  expect(visible.length).toBeLessThan(120);
  expect(visible.every((g) => g.x >= 20 && g.x <= 380)).toBe(true);
  expect(groups.every((g) => g.marks.every((p) => p.lane === g.lane))).toBe(true);
  expect(groupMarks(placed.map((p) => ({ ...p, x: p.x * 10 }))).length).toBeGreaterThan(groups.length);
});


it('reveals overlapping steps at closer zoom and gives selected steps room', () => {
  const mark = buildLanes(events, team)[0].marks[0];
  const placed: Placed[] = [0, 12, 15, 40].map((x, seq) => ({
    mark: { ...mark, seq }, lane: 0, x, labelled: false,
  }));
  expect(marksToDraw(groupMarks(placed), null).map((p) => p.mark.seq)).toEqual([0, 2, 3]);
  expect(marksToDraw(groupMarks(placed), 1).map((p) => p.mark.seq)).toEqual([1, 3]);
  const zoomed = placed.map((p) => ({ ...p, x: p.x * 5 }));
  expect(marksToDraw(groupMarks(zoomed), null).map((p) => p.mark.seq)).toEqual([0, 1, 2, 3]);
});
