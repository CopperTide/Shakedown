import { describe, expect, it } from 'vitest';
import { errorMessage } from './errors';

describe('errorMessage', () => {
  it('reads an Error, a thrown string, and anything with a message', () => {
    expect(errorMessage(new Error('shaked said no'))).toBe('shaked said no');
    expect(errorMessage('plain')).toBe('plain');
    expect(errorMessage({ message: 'from an object' })).toBe('from an object');
  });
  it('does not throw on nothing', () => {
    expect(errorMessage(undefined)).toBe('undefined');
    expect(errorMessage(null)).toBe('null');
  });
});
