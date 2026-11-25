import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { Me } from '@/api/me';
import { AccountMenu, choose, itemUrl } from './AccountMenu';

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
  it('a path is on the core, a full URL is used as it is', () => {
    expect(itemUrl('/api/switch', '/sd')).toBe('/sd/api/switch');
    expect(itemUrl('https://app.example.com/gw/api/logout', '/sd')).toBe('https://app.example.com/gw/api/logout');
  });
  it('posts a full URL as it is (a route of the deployment, outside the core), then reloads', async () => {
    const fetch = vi.fn(async () => new Response('{}'));
    const reload = vi.fn();
    await choose({ label: 'Sign out', post: 'https://app.example.com/gw/api/logout' }, { fetch, reload });
    expect(fetch).toHaveBeenCalledWith('https://app.example.com/gw/api/logout', expect.objectContaining({ method: 'POST', credentials: 'include' }));
    expect(reload).toHaveBeenCalledOnce();
  });
});
