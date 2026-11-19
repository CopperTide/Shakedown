import { describe, expect, it } from 'vitest';
import { path, qs } from './url';

describe('paths', () => {
  it('encodes each interpolated segment and nothing else', () => {
    expect(path`/api/branches/${'a/b c'}/rows`).toBe('/api/branches/a%2Fb%20c/rows');
    expect(path`/api/runs`).toBe('/api/runs');
    expect(path`/api/x/${3}`).toBe('/api/x/3');
  });
  it('builds a query in the order given, values encoded', () => {
    expect(qs({ seq: 4, keys: 'a/1,b 2' })).toBe('?seq=4&keys=a%2F1%2Cb%202');
    expect(qs({ a: 'x', b: 'y&z' })).toBe('?a=x&b=y%26z');
  });
});
