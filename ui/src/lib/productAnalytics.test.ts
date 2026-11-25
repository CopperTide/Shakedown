import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Me } from '@/api/me';

const ph = vi.hoisted(() => ({ init: vi.fn(), opt_in_capturing: vi.fn(), identify: vi.fn(), group: vi.fn(), reset: vi.fn(), opt_out_capturing: vi.fn(), stopSessionRecording: vi.fn(), capture: vi.fn() }));
vi.mock('posthog-js', () => ({ default: ph }));
const { choose } = await import('@/app/AccountMenu');
const { routeTemplate, startIfHosted, stopProductAnalytics, track, tracked } = await import('./productAnalytics');

const local: Me = { mode: 'local', user: { name: 'You' }, features: [] };
const hosted: Me = { mode: 'hosted', user: { name: 'Ana' }, features: [] };
const person = { id: 'u1', email: 'ana@x.com', name: 'Ana' };
const signedIn = { user: person, org: { id: 'o1', name: 'Ana' }, posthog: { key: 'phc_test', host: 'https://us.i.posthog.com' } };
const gateway = (status: number, body: unknown) => vi.fn(async () => new Response(JSON.stringify(body), { status }));

beforeEach(() => {
  stopProductAnalytics();
  vi.clearAllMocks();
});

describe('product analytics in hosted Shakedown', () => {
  it('a local shaked never asks the gateway, never loads PostHog and sends nothing', async () => {
    const get = gateway(200, signedIn);
    await startIfHosted(local, get);
    await startIfHosted(null, get);
    track('fork made');
    expect(get).not.toHaveBeenCalled();
    expect(ph.init).not.toHaveBeenCalled();
    expect(ph.capture).not.toHaveBeenCalled();
  });

  it('hosted without a key, or signed out at the gateway, starts nothing', async () => {
    await startIfHosted(hosted, gateway(200, { ...signedIn, posthog: null }));
    await startIfHosted(hosted, gateway(401, {}));
    await startIfHosted(hosted, vi.fn(async () => { throw new Error('offline'); }));
    expect(ph.init).not.toHaveBeenCalled();
  });

  it('hosted and signed in with a key: identify the user, group the org, then count the core flow', async () => {
    const get = gateway(200, signedIn);
    await startIfHosted(hosted, get);
    expect(get).toHaveBeenCalledWith('/gw/api/me');
    expect(ph.init).toHaveBeenCalledWith('phc_test', expect.objectContaining({ api_host: 'https://us.i.posthog.com', respect_dnt: true }));
    expect(ph.identify).toHaveBeenCalledWith('u1', { email: 'ana@x.com', name: 'Ana' });
    expect(ph.group).toHaveBeenCalledWith('organization', 'o1', { name: 'Ana' });
    expect(tracked('fork made')({ id: 'b2' })).toEqual({ id: 'b2' });
    track('compare opened', { of: 'runs' });
    expect(ph.capture.mock.calls).toEqual([['fork made', undefined], ['compare opened', { of: 'runs' }]]);
  });

  it('signing out from the account menu resets PostHog', async () => {
    await startIfHosted(hosted, gateway(200, signedIn));
    await choose({ label: 'Sign out', post: 'https://app.example.com/gw/api/logout' }, { fetch: vi.fn(async () => new Response('{}')), reload: vi.fn() });
    expect(ph.reset).toHaveBeenCalledOnce();
    track('run started');
    expect(ph.capture).not.toHaveBeenCalled();
  });

  it('page addresses keep only fixed words', () => {
    expect(routeTemplate('/worlds/acme-plant')).toBe('/worlds/:id');
    expect(routeTemplate('/worlds/new')).toBe('/worlds/new');
    expect(routeTemplate('/experiments/file/secret-launch')).toBe('/experiments/file/:id');
    expect(routeTemplate('/runs/r1/b2')).toBe('/runs/:id/:id');
    expect(routeTemplate('/somewhere/secret')).toBe('/:id/:id');
  });
});
