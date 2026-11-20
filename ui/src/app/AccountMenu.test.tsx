import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { Me } from '@/api/me';
import { AccountMenu, choose } from './AccountMenu';

const local: Me = { mode: 'local', user: { name: 'You' }, features: [] };

describe('Account menu', () => {
  it('shows no account menu when /api/me has no account', () => {
    expect(renderToStaticMarkup(<AccountMenu me={local} />)).toBe('');
    expect(renderToStaticMarkup(<AccountMenu me={null} />)).toBe('');
  });
  it('shows the label the layer gives', () => {
    const me: Me = { ...local, account: { label: 'Acme', items: [{ label: 'Acme', current: true }] } };
    expect(renderToStaticMarkup(<AccountMenu me={me} />)).toContain('Acme');
  });
  it('posts the chosen item of the account menu and reloads', async () => {
    const fetch = vi.fn(async () => new Response('{}'));
    const reload = vi.fn();
    await choose({ label: 'Other', post: '/api/switch', body: { to: 'b' } }, { fetch, reload });
    expect(fetch).toHaveBeenCalledWith(expect.stringMatching(/\/api\/switch$/), {
      method: 'POST', credentials: 'include', headers: { 'content-type': 'application/json' }, body: '{"to":"b"}',
    });
    expect(reload).toHaveBeenCalledOnce();
    fetch.mockClear();
    await choose({ label: 'Here', current: true }, { fetch, reload });
    expect(fetch).not.toHaveBeenCalled();
  });
});
