import { expect, it } from 'vitest';
import { events, team } from '@/model/fixtures';
import { buildLanes } from '@/model/lanes';
import type { Layout } from './timelineLayout';
import { indexTimeline, viewportTimeline } from './timelineViewport';

function layout(xs: number[]): Layout {
  const mark = buildLanes(events, team)[0].marks[0];
  return { placed: xs.map((x, seq) => ({ mark: { ...mark, seq }, lane: seq % 4, x, labelled: false })), connectors: [], ticks: [], bars: [], bands: [], gaps: [] };
}
it('keeps every overlapping event in view, and every event at fit zoom', () => {
  const l = layout(Array.from({ length: 20000 }, () => 400));
  const index = indexTimeline(l);
  expect(viewportTimeline(index, 0, 800, null).placed).toHaveLength(20000);
  expect(viewportTimeline(index, 2000, 800, null).placed).toHaveLength(0);
  expect(viewportTimeline(index, 0, 800, null).placed).toHaveLength(20000);
});
it('mounts a small window of a long timeline and restores events when panning', () => {
  const index = indexTimeline(layout(Array.from({ length: 20000 }, (_, i) => i * 10)));
  const window = viewportTimeline(index, 50000, 800, null);
  expect(window.placed.length).toBeLessThan(120);
  expect(window.placed.some((p) => p.mark.seq === 5000)).toBe(true);
  expect(window.placed.some((p) => p.mark.seq === 0)).toBe(false);
  expect(viewportTimeline(index, 0, 800, null).placed.some((p) => p.mark.seq === 0)).toBe(true);
  expect(viewportTimeline(index, 50000, 800, 0).placed.some((p) => p.mark.seq === 0)).toBe(true);
});
it('keeps bands crossing the viewport and labels reaching in from its left edge', () => {
  const l = layout([0, 800, 2000]);
  l.placed[0].labelled = true;
  l.placed[0].mark = { ...l.placed[0].mark, kind: 'world', label: 'Long label '.repeat(20) };
  l.bands = [{ x1: 0, x2: 3000, open: false }, { x1: 2500, x2: 2600, open: false }];
  l.connectors = [{ seq: 1, x: 800, from: 0, to: 1 }, { seq: 2, x: 2000, from: 0, to: 1 }];
  const visible = viewportTimeline(indexTimeline(l), 700, 500, null);
  expect(visible.placed.map((p) => p.mark.seq)).toEqual([0, 1]);
  expect(visible.bands).toHaveLength(1);
  expect(visible.connectors.map((c) => c.seq)).toEqual([1]);
});
