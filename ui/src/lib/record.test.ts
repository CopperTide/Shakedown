import { describe, expect, it } from 'vitest';
import { asRecord } from './record';

describe('asRecord', () => {
  it('passes objects through and turns anything else into an empty record', () => {
    const o = { a: 1 };
    expect(asRecord(o)).toBe(o);
    expect([asRecord(null), asRecord(undefined), asRecord('x'), asRecord(3)]).toEqual([{}, {}, {}, {}]);
  });
});
