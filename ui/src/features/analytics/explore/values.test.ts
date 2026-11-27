import { describe, expect, it } from 'vitest';
import { csvField, dimensionText, measureText, toCsv } from './values';

describe('Explore values', () => {
  it('formats a measure by its unit', () => {
    expect(measureText(0.5612, 'usd')).toBe('$0.56');
    expect(measureText(2 / 3, 'ratio')).toBe('67%');
    expect(measureText(186, 'seconds')).toBe('3.1 min');
    expect(measureText(1234, 'count')).toBe((1234).toLocaleString());
    expect(measureText(null, 'usd')).toBe('–');
  });

  it('names an experiment, and says (none) for no value', () => {
    expect(dimensionText('x1', 'experiment', { experiment: { x1: 'Hub day' } })).toBe('Hub day');
    expect(dimensionText(null, 'agent', {})).toBe('(none)');
  });

  it('writes CSV with quoting for commas, quotes and line breaks', () => {
    expect(csvField('a,b')).toBe('"a,b"');
    expect(csvField('say "hi"')).toBe('"say ""hi"""');
    expect(csvField('two\nlines')).toBe('"two\nlines"');
    expect(csvField(null)).toBe('');
    expect(csvField({ k: 1 })).toBe('"{""k"":1}"');
    expect(toCsv(['setup', 'runs'], [['Team (a, b)', 3], ['Solo', null]])).toBe('setup,runs\r\n"Team (a, b)",3\r\nSolo,\r\n');
  });
});
