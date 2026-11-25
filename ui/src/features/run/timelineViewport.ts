import { shortLabel, type Layout, type Placed } from './timelineLayout';

const BUFFER = 160;
/** Index once per layout/zoom, not once per scroll frame. */
export function indexTimeline(layout: Layout) {
  return {
    layout,
    placed: [...layout.placed].sort((a, b) => a.x - b.x),
    connectors: [...layout.connectors].sort((a, b) => a.x - b.x),
    bySeq: new Map(layout.placed.map((p) => [p.mark.seq, p])),
    labelReach: layout.placed.reduce((max, p) => Math.max(max, p.labelled ? 24 + shortLabel(p.mark).length * 7 : 12), 12),
  };
}
function lowerBound(items: { x: number }[], x: number, inclusive = false) {
  let lo = 0, hi = items.length;
  while (lo < hi) {
    const mid = (lo + hi) >>> 1;
    if (items[mid].x < x || (inclusive && items[mid].x === x)) lo = mid + 1; else hi = mid;
  }
  return lo;
}
function range<T extends { x: number }>(items: T[], left: number, right: number): T[] {
  return items.slice(lowerBound(items, left), lowerBound(items, right, true));
}
/** Only cull offscreen geometry. Every overlapping event in view remains a separate marker. */
export function viewportTimeline(index: ReturnType<typeof indexTimeline>, scroll: number, width: number, selected: number | null): Layout {
  const left = scroll - BUFFER, right = scroll + width + BUFFER;
  const placed = range(index.placed, left - index.labelReach, right);
  // Keep the selected DOM node mounted while keyboard navigation scrolls it into view.
  const active: Placed | undefined = selected == null ? undefined : index.bySeq.get(selected);
  if (active && (active.x < left - index.labelReach || active.x > right)) placed.push(active);
  const intersects = (b: { x1: number; x2: number }) => b.x2 >= left && b.x1 <= right;
  return {
    placed, connectors: range(index.connectors, left, right),
    ticks: index.layout.ticks.filter((t) => t.x >= left && t.x <= right),
    bars: index.layout.bars.filter(intersects), gaps: index.layout.gaps.filter(intersects), bands: index.layout.bands.filter(intersects),
  };
}
