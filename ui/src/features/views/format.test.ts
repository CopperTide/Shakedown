import { describe, expect, it } from 'vitest';
import { DASH, formatValue, isNumericFormat, minutes, toMs, utcDate, utcTime } from './format';
import { cols, columnOf, fieldNames, toNum } from './rows';

const T = Date.UTC(2026, 10, 22, 14, 5, 30);

describe('formats', () => {
  it('formats_numbers_by_kind', () => {
    expect(formatValue(1234.6, 'int')).toBe('1,235');
    expect(formatValue(12.345, 'number', 2)).toBe('12.35');
    expect(formatValue(7, 'number')).toBe('7');
    expect(formatValue(81.24, 'percent')).toBe('81.2%');
    expect(formatValue(0.93, 'ratio_pct')).toBe('93%');
    expect(formatValue(0.934, 'ratio_pct', 1)).toBe('93.4%');
    expect(formatValue(1840.5, 'usd')).toBe('$1,841');
    expect(formatValue(12.5, 'usd')).toBe('$12.50');
    expect(formatValue(-30, 'usd')).toBe('-$30.00');
  });

  it('formats_minutes_as_hours_and_minutes', () => {
    expect(minutes(75)).toBe('1:15');
    expect(minutes(5)).toBe('0:05');
    expect(minutes(-20)).toBe('-0:20');
    expect(formatValue(130, 'minutes')).toBe('2:10');
  });

  it('formats_times_in_utc', () => {
    expect(utcTime(T)).toBe('14:05Z');
    expect(utcDate(T)).toBe('Sun 22 Nov 2026');
    expect(formatValue(T, 'time')).toBe('14:05Z');
    expect(formatValue('2026-11-22T09:30:00Z', 'time')).toBe('09:30Z');
    expect(toMs('not a time')).toBeNull();
  });

  it('chip_shows_its_label_else_the_value', () => {
    expect(formatValue('held', 'chip', undefined, 'Held')).toBe('Held');
    expect(formatValue('held', 'chip')).toBe('held');
    expect(formatValue(null, 'chip')).toBe(DASH);
  });

  it('nothing_reads_as_a_dash_and_odd_values_as_text', () => {
    expect(formatValue(null, 'int')).toBe(DASH);
    expect(formatValue(undefined)).toBe(DASH);
    expect(formatValue('n/a', 'int')).toBe('n/a');
    expect(formatValue({ a: 1 })).toBe('{"a":1}');
    expect(formatValue('a1')).toBe('a1');
  });

  it('numeric_formats', () => {
    expect(isNumericFormat('int')).toBe(true);
    expect(isNumericFormat('text')).toBe(false);
    expect(isNumericFormat('chip')).toBe(false);
    expect(isNumericFormat(undefined)).toBe(false);
  });
});

describe('rows', () => {
  it('reads_by_column_name_not_position', () => {
    const c = cols(['key', 'extra', 'value']);
    const row = ['a1', 'x', 3];
    expect(c.get(row, 'value')).toBe(3);
    expect(c.num(row, 'value')).toBe(3);
    expect(c.str(row, 'key')).toBe('a1');
    expect(c.get(row, 'missing')).toBeUndefined();
    expect(c.str(row, 'missing')).toBe('');
  });

  it('a_field_list_matches_its_joined_or_first_present_name', () => {
    expect(fieldNames('a.b')).toEqual(['a.b']);
    expect(fieldNames(['on', 'eta'])).toEqual(['on|eta', 'on', 'eta']);
    expect(columnOf(cols(['key', 'on']), ['on', 'eta'])).toBe('on');
    expect(columnOf(cols(['key']), 'x')).toBeNull();
  });

  it('to_num', () => {
    expect(toNum('4.5')).toBe(4.5);
    expect(toNum('')).toBeNull();
    expect(toNum(Number.NaN)).toBeNull();
    expect(toNum(true)).toBeNull();
  });
});
