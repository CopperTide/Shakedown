import { describe, expect, it } from 'vitest';
import { boardColumns, boardRows, cellText, columnWidths, defaultSort, filterCounts, nextSort } from './board';
import { cols } from './rows';
import { barsByLane, rangeOf, ticks, xOf, zoomStep } from './timeline';
import type { BoardData, BoardDef, TimelineData } from './types';
import { windowOf } from './virtual';

const def: BoardDef = {
  kind: 'board', title: 'Items',
  columns: [{ field: 'id', label: 'Item' }, { field: 'n', label: 'Count', format: 'int' }, { field: 'at', label: 'At', format: 'time' }, { field: 'status', label: 'Status', format: 'chip' }, { field: 'gone', label: 'Gone' }],
  sort: { field: 'n', desc: true },
  filters: [{ id: 'f1', label: 'Odd' }, { id: 'f2', label: 'Big' }],
};
const H = 3_600_000;
// Columns in an order the def doesn't follow, plus one the def doesn't name.
const data: BoardData = {
  kind: 'board', total: 9,
  columns: ['key', 'extra', 'status', 'n', 'id', 'at', 'tone', 'chip', 'filters'],
  rows: [
    ['a1', 0, 'ok', 5, 'a1', 2 * H, 'neutral', null, 1],
    ['a2', 0, 'held', 12, 'a2', 1 * H, 'critical', 'Held', 2],
    ['a3', 0, 'ok', null, 'b3', 3 * H, 'neutral', null, 3],
    ['a4', 0, 'ok', 2, 'a4', 0, 'warn', 'Late', 0],
  ],
};

describe('board', () => {
  const c = cols(data.columns);
  const columns = boardColumns(def, c);

  it('maps_def_columns_to_payload_columns_by_name', () => {
    expect(columns.map((x) => x.name)).toEqual(['id', 'n', 'at', 'status', null]);
    expect(columns.map((x) => x.numeric)).toEqual([false, true, false, false, false]);
    expect(cellText(c, data.rows[1], columns[3])).toBe('Held');
    expect(cellText(c, data.rows[0], columns[3])).toBe('ok');
    expect(cellText(c, data.rows[1], columns[2])).toBe('01:00Z');
    expect(cellText(c, data.rows[1], columns[4])).toBe('–');
  });

  it('sorts_by_the_def_default_nulls_last', () => {
    const sort = defaultSort(def, columns, c);
    expect(sort).toEqual({ column: 1, desc: true });
    expect(boardRows(data, columns, { sort, filters: [], search: '' }).map((i) => data.rows[i][0])).toEqual(['a2', 'a1', 'a4', 'a3']);
    expect(boardRows(data, columns, { sort: { column: 1, desc: false }, filters: [], search: '' }).map((i) => data.rows[i][0])).toEqual(['a4', 'a1', 'a2', 'a3']);
    expect(boardRows(data, columns, { sort: { column: 2, desc: false }, filters: [], search: '' }).map((i) => data.rows[i][0])).toEqual(['a4', 'a2', 'a1', 'a3']);
  });

  it('quick_filters_by_bitmask_any_of_the_ones_on', () => {
    expect(boardRows(data, columns, { sort: null, filters: [0], search: '' }).map((i) => data.rows[i][0])).toEqual(['a1', 'a3']);
    expect(boardRows(data, columns, { sort: null, filters: [0, 1], search: '' }).map((i) => data.rows[i][0])).toEqual(['a1', 'a2', 'a3']);
    expect(filterCounts(data, 2)).toEqual([2, 2]);
  });

  it('searches_text_columns_and_chips', () => {
    expect(boardRows(data, columns, { sort: null, filters: [], search: 'B3' }).map((i) => data.rows[i][0])).toEqual(['a3']);
    expect(boardRows(data, columns, { sort: null, filters: [], search: 'late' }).map((i) => data.rows[i][0])).toEqual(['a4']);
    expect(boardRows(data, columns, { sort: null, filters: [], search: '12' })).toEqual([]);
  });

  it('cycles_sort_on_header_clicks', () => {
    expect(nextSort(null, 0, false)).toEqual({ column: 0, desc: false });
    expect(nextSort({ column: 0, desc: false }, 0, false)).toEqual({ column: 0, desc: true });
    expect(nextSort({ column: 0, desc: true }, 0, false)).toBeNull();
    expect(nextSort(null, 1, true)).toEqual({ column: 1, desc: true });
  });

  it('sizes_columns_from_their_text', () => {
    const w = columnWidths(data, columns);
    expect(w).toHaveLength(5);
    expect(w.every((x) => x >= 4 && x <= 40)).toBe(true);
    expect(w[3]).toBeGreaterThanOrEqual('Status'.length + 2);
  });
});

describe('windowing', () => {
  it('draws_only_the_rows_in_view', () => {
    const w = windowOf(2200, 440, 22, 2500, 5);
    expect(w.start).toBe(95);
    expect(w.end).toBe(126);
    expect(w.before).toBe(95 * 22);
    expect(w.after).toBe((2500 - 126) * 22);
    expect(windowOf(0, 100, 22, 0)).toEqual({ start: 0, end: 0, before: 0, after: 0 });
    expect(windowOf(-50, 100, 20, 3).end).toBe(3);
  });
});

const tl: TimelineData = {
  kind: 'timeline', lanes: ['u1', 'u2'], range: [0, 0],
  columns: ['key', 'lane', 'label', 'start', 'end', 'tone', 'chip'],
  rows: [['k2', 0, 'B', 5 * H, 6 * H, 'warn', '+20'], ['k1', 0, 'A', 1 * H, 2 * H, 'active', null], ['k3', 1, 'C', 2 * H, null, 'oops', null], ['k4', 7, 'D', 0, 1, 'neutral', null], ['k5', 1, 'E', null, H, 'neutral', null]],
};

describe('timeline', () => {
  it('groups_bars_by_lane_index_in_start_order', () => {
    const lanes = barsByLane(tl);
    expect(lanes.map((l) => l.map((b) => b.key))).toEqual([['k1', 'k2'], ['k3']]);
    expect(lanes[1][0]).toMatchObject({ end: 2 * H, tone: 'neutral' });
  });

  it('range_from_payload_else_the_bars', () => {
    const lanes = barsByLane(tl);
    expect(rangeOf(tl, lanes, null)).toEqual([1 * H, 6 * H]);
    expect(rangeOf({ ...tl, range: [0, 10 * H] }, lanes, null)).toEqual([0, 10 * H]);
    expect(rangeOf({ ...tl, rows: [] }, [[], []], 5 * H)).toEqual([5 * H - 1_800_000, 5 * H + 1_800_000]);
  });

  it('ticks_on_round_utc_times', () => {
    const ts = ticks(0, 24 * H, 1200);
    expect(ts[0]).toMatchObject({ ms: 0, label: '00:00', major: true });
    expect(ts[1].ms - ts[0].ms).toBe(2 * H);
    expect(ticks(0, H, 1200)[1].label).toBe('00:05');
    expect(ticks(5, 5, 100)).toEqual([]);
  });

  it('x_and_zoom', () => {
    expect(xOf(5, 0, 10, 200)).toBe(100);
    expect(zoomStep(1, 1)).toBe(1.5);
    expect(zoomStep(1, -1)).toBe(1);
    expect(zoomStep(16, 1)).toBe(16);
    expect(zoomStep(5, -1)).toBe(4);
  });
});
