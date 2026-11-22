// Fixed-height windowing: which rows (or lanes) of a long list to draw for a scroll position.

export interface Window {
  start: number;
  /** One past the last row drawn. */
  end: number;
  /** Space above the first row drawn, in px. */
  before: number;
  /** Space below the last row drawn, in px. */
  after: number;
}

export function windowOf(scrollTop: number, viewport: number, rowHeight: number, count: number, overscan = 8): Window {
  if (count <= 0 || rowHeight <= 0) return { start: 0, end: 0, before: 0, after: 0 };
  const first = Math.floor(Math.max(0, scrollTop) / rowHeight);
  const visible = Math.ceil(Math.max(viewport, rowHeight) / rowHeight) + 1;
  const start = Math.max(0, Math.min(count - 1, first - overscan));
  const end = Math.min(count, first + visible + overscan);
  return { start, end, before: start * rowHeight, after: Math.max(0, (count - end) * rowHeight) };
}
