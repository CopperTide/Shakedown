import type { Placed } from './timelineLayout';

export interface MarkGroup { marks: Placed[]; x: number; lane: number }
/** Fixed screen-space buckets avoid overlapping targets; positions stay stable while panning. */
export function groupMarks(placed: Placed[], cell = 32): MarkGroup[] {
  const groups = new Map<string, MarkGroup>();
  for (const p of placed) {
    const key = `${p.lane}:${Math.floor(p.x / cell)}`;
    const group = groups.get(key);
    if (group) {
      group.marks.push(p);
      group.x = (Math.floor(p.x / cell) + 0.5) * cell;
    }
    else groups.set(key, { marks: [p], x: p.x, lane: p.lane });
  }
  return [...groups.values()];
}
/** Keep a small overscan so targets do not flash in and out at the viewport edge. */
export function visibleGroups(groups: MarkGroup[], scroll: number, width: number) {
  return groups.filter((g) => g.x >= scroll - 80 && g.x <= scroll + width + 80);
}
