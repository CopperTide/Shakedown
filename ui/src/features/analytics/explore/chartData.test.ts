import { describe, expect, it } from 'vitest';
import type { QueryResult, SqlResult } from '@/api/explore';
import { builderChart, defaultAxes, MAX_SERIES, sqlChart, stackBases, topOf } from './chartData';

const col = (id: string, role: 'dimension' | 'measure', unit?: 'usd' | 'count') => ({ id, label: id.toUpperCase(), role, unit });

describe('builder charts', () => {
  it('one dimension: one series in row order, experiments by name', () => {
    const r: QueryResult = { columns: [col('experiment', 'dimension'), col('cost', 'measure', 'usd')], rows: [['x1', 2], [null, 1]], labels: { experiment: { x1: 'Hub day' } }, truncated: false, ms: 3 };
    const d = builderChart(r, 'cost');
    expect(d.labels).toEqual(['Hub day', '(none)']);
    expect(d.keys).toEqual(['x1', null]);
    expect(d.series).toEqual([{ name: 'COST', key: null, values: [2, 1] }]);
    expect(d.unit).toBe('usd');
  });

  it('two dimensions: a series per second value, biggest first, gaps as null', () => {
    const r: QueryResult = {
      columns: [col('day', 'dimension'), col('agent', 'dimension'), col('cost', 'measure')],
      rows: [['d1', 'a', 1], ['d1', 'b', 5], ['d2', 'b', 2]], labels: {}, truncated: false, ms: 1,
    };
    const d = builderChart(r, 'cost');
    expect(d.labels).toEqual(['d1', 'd2']);
    expect(d.series.map((s) => [s.name, s.values])).toEqual([['b', [5, 2]], ['a', [1, null]]]);
    expect(stackBases(d)).toEqual([[0, 0], [5, 2]]);
    expect(topOf(d, true)).toBe(6);
    expect(topOf(d, false)).toBe(5);
  });

  it('caps the series and says how many are left out', () => {
    const rows = Array.from({ length: MAX_SERIES + 3 }, (_, i) => ['x', `s${i}`, i + 1]);
    const r: QueryResult = { columns: [col('tool', 'dimension'), col('agent', 'dimension'), col('runs', 'measure')], rows, labels: {}, truncated: false, ms: 1 };
    const d = builderChart(r, 'runs');
    expect(d.series).toHaveLength(MAX_SERIES);
    expect(d.hidden.series).toBe(3);
  });

  it('no dimension: one bar of the total', () => {
    const r: QueryResult = { columns: [col('runs', 'measure')], rows: [[7]], labels: {}, truncated: false, ms: 1 };
    expect(builderChart(r, 'runs').series[0].values).toEqual([7]);
  });
});

describe('SQL charts', () => {
  const r: SqlResult = {
    sql: 'q', truncated: null, ms: 1,
    columns: [{ name: 'setup', type: 'text', kind: 'text' }, { name: 'runs', type: 'int8', kind: 'number' }, { name: 'cost', type: 'float8', kind: 'number' }],
    rows: [['a', 3, '1.5'], ['b', 1, null]],
  };

  it('picks a text column for x and the numbers as series', () => {
    expect(defaultAxes(r)).toEqual({ x: 'setup', ys: ['runs', 'cost'] });
    const d = sqlChart(r, 'setup', ['runs', 'cost']);
    expect(d.labels).toEqual(['a', 'b']);
    expect(d.series.map((s) => s.values)).toEqual([[3, 1], [1.5, null]]);
  });
});

describe('a result from before the measure changed', () => {
  it('charts its first measure instead of failing', () => {
    const r: QueryResult = { columns: [col('agent', 'dimension'), col('cost', 'measure')], rows: [['a', 1]], labels: {}, truncated: false, ms: 1 };
    expect(builderChart(r, 'refusals').series[0]).toMatchObject({ name: 'COST', values: [1] });
  });
});
