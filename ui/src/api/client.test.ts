import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiError, del, get, path, post, qs } from './client';

afterEach(() => vi.unstubAllGlobals());

/** A fetch that answers `body` with `status`, and remembers what it was asked. */
function answer(body: string, status = 200) {
  const f = vi.fn(async (_url: string, _init?: RequestInit) => new Response(body, { status, statusText: status === 200 ? 'OK' : 'Bad' }));
  vi.stubGlobal('fetch', f);
  return f;
}

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

describe('requests', () => {
  it('get reads JSON; an empty body is null', async () => {
    answer('{"ok":true}');
    expect(await get('/api/health')).toEqual({ ok: true });
    answer('');
    expect(await get('/api/health')).toBeNull();
  });
  it('post sends JSON, or a bare POST without a body', async () => {
    const f = answer('{}');
    await post('/api/runs', { task_id: 't' });
    expect(f.mock.calls[0][1]).toEqual({ method: 'POST', headers: { 'content-type': 'application/json' }, body: '{"task_id":"t"}' });
    await post('/api/review');
    expect(f.mock.calls[1][1]).toEqual({ method: 'POST' });
  });
  it('del sends DELETE', async () => {
    const f = answer('{"dropped":1}');
    expect(await del('/api/runs/r1')).toEqual({ dropped: 1 });
    expect(f.mock.calls[0][1]).toEqual({ method: 'DELETE' });
  });
  it('a failed request throws ApiError with arenad’s error, else the text, else the status text', async () => {
    answer('{"error":"no such run"}', 404);
    await expect(get('/api/runs/x')).rejects.toEqual(new ApiError(404, 'no such run'));
    answer('plain words', 500);
    await expect(get('/api/x')).rejects.toMatchObject({ status: 500, message: 'plain words' });
    answer('', 500);
    await expect(get('/api/x')).rejects.toMatchObject({ status: 500, message: 'Bad' });
  });
});
