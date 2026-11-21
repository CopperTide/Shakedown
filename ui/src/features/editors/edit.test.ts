// Draft updates are pure: they return a changed copy and leave the original alone.
import { describe, expect, it } from 'vitest';
import { addTo, removeAt, setIn } from './edit';
import { isValidId } from './idRule';

describe('edit helpers', () => {
  it('set_in_updates_a_copy', () => {
    const before = { a: { b: [1, { c: 'x' }] } };
    const after = setIn(before, ['a', 'b', 1, 'c'], 'y');
    expect(after).toEqual({ a: { b: [1, { c: 'y' }] } });
    expect(before.a.b[1]).toEqual({ c: 'x' });
  });

  it('set_in_removes_undefined', () => {
    expect(setIn({ at: '07:30', after_calls: 3 }, ['at'], undefined)).toEqual({ after_calls: 3 });
  });

  it('lists', () => {
    expect(addTo([1], 2)).toEqual([1, 2]);
    expect(removeAt([1, 2, 3], 1)).toEqual([1, 3]);
  });

  it('id_rule', () => {
    expect(isValidId('larkspur-plant')).toBe(true);
    for (const bad of ['My World', '', '-x', 'a/b', 'x'.repeat(65)]) expect(isValidId(bad)).toBe(false);
  });
});
