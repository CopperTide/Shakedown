import type { Placed } from './timelineLayout';

export interface MarkGroup { marks: Placed[]; x: number; lane: number }
/** Keep one target per visible interval in each sorted lane; zoom reveals the remaining events. */
export function groupMarks(placed: Placed[], cell = 14): MarkGroup[] {
  const out: MarkGroup[] = [];
  const groups = new Map<string, MarkGroup>();
  for (const p of placed) {
    const key = String(p.lane);
    const group = groups.get(key);
    if (group && p.x - group.x < cell) {
      group.marks.push(p);
    } else {
      const next = { marks: [p], x: p.x, lane: p.lane };
      groups.set(key, next);
      out.push(next);
    }
  }
  return out;
}
/** Keep a small overscan so targets do not flash in and out at the viewport edge. */
export function visibleGroups(groups: MarkGroup[], scroll: number, width: number) {
  return groups.filter((g) => g.x >= scroll - 80 && g.x <= scroll + width + 80);
}

/** Selection always wins over nearby marks, without changing their original positions. */
export function marksToDraw(groups: MarkGroup[], selected: number | null, spacing = 14): Placed[] {
  const representatives = groups.map((g) => (selected == null ? undefined : g.marks.find((p) => p.mark.seq === selected)) ?? g.marks[0]);
  const active = representatives.find((p) => p.mark.seq === selected);
  return representatives.filter((p) => !active || p === active || p.lane !== active.lane || Math.abs(p.x - active.x) >= spacing);
}
