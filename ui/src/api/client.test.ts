import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiError, del, get, post } from './client';
import { resetCsrf } from './csrf';

afterEach(() => { vi.unstubAllGlobals(); resetCsrf(); });

/** A fetch that answers `body` with `status`, and remembers what it was asked. */
function answer(body: string, status = 200) {
  const f = vi.fn(async (_url: string, _init?: RequestInit) => new Response(body, { status, statusText: status === 200 ? 'OK' : 'Bad' }));
  vi.stubGlobal('fetch', f);
  return f;
}

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
  it('a failed request throws ApiError with shaked’s error, else the text, else the status text', async () => {
    answer('{"error":"no such run"}', 404);
    await expect(get('/api/runs/x')).rejects.toEqual(new ApiError(404, 'no such run'));
    answer('plain words', 500);
    await expect(get('/api/x')).rejects.toMatchObject({ status: 500, message: 'plain words' });
    answer('', 500);
    await expect(get('/api/x')).rejects.toMatchObject({ status: 500, message: 'Bad' });
  });
});

describe('a refusal', () => {
  it('keeps the JSON shaked answered, for answers that say more than an error', async () => {
    answer('{"problems":[{"path":"company.name","message":"Give the company a name"}]}', 422);
    await expect(get('/api/x')).rejects.toMatchObject({ status: 422, body: { problems: [{ path: 'company.name', message: 'Give the company a name' }] } });
  });
});

describe('the write token', () => {
  it('a token an answer carried goes back on the next write, not on reads', async () => {
    const f = vi.fn(async (_url: string, _init?: RequestInit) => new Response('{}', { headers: { 'x-csrf-token': 'tok' } }));
    vi.stubGlobal('fetch', f);
    await get('/api/me');
    await post('/api/runs', { task_id: 't' });
    await get('/api/runs');
    expect(new Headers(f.mock.calls[1][1]?.headers).get('x-csrf-token')).toBe('tok');
    expect(new Headers(f.mock.calls[2][1]?.headers).get('x-csrf-token')).toBeNull();
  });
});

