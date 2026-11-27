// The project client: puts send the body and confirm, and "no project" (409) is a state.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { loadProject } from '@/hooks/useProject';
import { putItem } from './project';

afterEach(() => vi.unstubAllGlobals());

describe('project client', () => {
  it('put_sends_confirm', async () => {
    const calls: { url: string; init: RequestInit }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (url: string, init: RequestInit) => {
      calls.push({ url, init });
      return new Response(JSON.stringify({ id: 'x' }), { status: 200 });
    }));
    await putItem('scenarios', 'supplier-slip', { label: 'S' });
    expect(calls[0].url).toMatch(/\/api\/project\/scenarios\/supplier-slip$/);
    expect(calls[0].init.method).toBe('PUT');
    expect(JSON.parse(String(calls[0].init.body))).toEqual({ body: { label: 'S' }, confirm: true });
  });

  it('status_409_is_not_open', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ error: 'no project is open' }), { status: 409 })));
    expect(await loadProject()).toEqual({ open: false, message: 'no project is open', folder: false });
  });

  it('status_409_says_whether_projects_are_folders', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ error: 'no project is open', folder: true }), { status: 409 })));
    expect(await loadProject()).toEqual({ open: false, message: 'no project is open', folder: true });
  });
});
