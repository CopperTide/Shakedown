import { describe, expect, it } from 'vitest';
import { ago } from './time';

describe('age', () => {
  it('says how long ago in words', () => {
    const now = Date.parse('2026-11-06T12:00:00Z');
    expect(ago('2026-11-06T11:59:30Z', now)).toBe('just now');
    expect(ago('2026-11-06T11:15:00Z', now)).toBe('45 min ago');
    expect(ago('2026-11-06T09:00:00Z', now)).toBe('3 h ago');
    expect(ago('not a date', now)).toBe('not a date');
  });
});
