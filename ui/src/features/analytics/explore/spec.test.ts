import { describe, expect, it } from 'vitest';
import { apiSpec, chartedMeasure, DEFAULT_STATE, fromSaved, parseBuilder, PRESETS, savedSpec, withPreset, writeBuilder } from './spec';

describe('the builder in the address', () => {
  it('opens on the default query when no measure is given', () => {
    expect(parseBuilder(new URLSearchParams('tab=explore'))).toEqual(DEFAULT_STATE);
  });

  it('round-trips measures, dimensions, filters, chart and charted measure', () => {
    const s = { ...DEFAULT_STATE, measures: ['refusals', 'tool_calls'] as const, dimensions: ['tool', 'agent'] as const, range: '7d' as const,
      setup: ['Team (lead, two helpers)'], agent: ['a1'], experiment: ['x1'], chart: 'stacked' as const, y: 'tool_calls' as const };
    const sp = writeBuilder(new URLSearchParams('tab=explore'), { ...s, measures: [...s.measures], dimensions: [...s.dimensions] });
    expect(sp.get('tab')).toBe('explore');
    expect(parseBuilder(sp)).toEqual({ ...s, measures: [...s.measures], dimensions: [...s.dimensions] });
  });

  it('drops unknown names, repeats and a third dimension; leaves defaults out', () => {
    const s = parseBuilder(new URLSearchParams('m=cost&m=cost&m=nope&d=agent&d=bogus&d=model&d=tool&chart=pie&range=1y'));
    expect(s.measures).toEqual(['cost']);
    expect(s.dimensions).toEqual(['agent', 'model']);
    expect(s.chart).toBe('bar');
    expect(s.range).toBe('30d');
    const sp = writeBuilder(new URLSearchParams(), s);
    expect([...sp.keys()].sort()).toEqual(['d', 'd', 'm']);
  });

  it('charts the picked measure only while it is shown', () => {
    expect(chartedMeasure({ ...DEFAULT_STATE, y: 'runs' })).toBe('runs');
    expect(chartedMeasure({ ...DEFAULT_STATE, y: 'errors' })).toBe('cost');
  });
});

describe('the API spec', () => {
  it('turns the range into a start time and leaves empty filters out', () => {
    const now = new Date('2026-11-26T15:00:00Z');
    const spec = apiSpec({ ...DEFAULT_STATE, range: '7d', setup: ['s'] }, now);
    expect(spec).toEqual({ measures: ['cost', 'runs'], dimensions: ['agent'], filters: { from: '2026-11-20T00:00:00.000Z', setup: ['s'] } });
    expect(apiSpec({ ...DEFAULT_STATE, range: 'all' }, now).filters).toEqual({});
  });

  it('a saved view keeps the range, not a start time, and opens again as it was', () => {
    const s = withPreset({ ...DEFAULT_STATE, range: '90d', agent: ['a'] }, PRESETS[1]);
    const saved = savedSpec(s);
    expect(saved.filters).toEqual({ agent: ['a'] });
    expect(saved.range).toBe('90d');
    expect(fromSaved(saved)).toEqual(s);
  });
});
