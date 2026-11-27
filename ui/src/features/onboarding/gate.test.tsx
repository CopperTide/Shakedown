// @vitest-environment jsdom
// The onboarding gate: every page but Get started sends you there until setup (a project and a
// model) is done, by what shaked says exists. Worlds, agents and scenarios don't hold it shut.
import { cleanup, render, screen } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { forgetShared } from '@/hooks/query';
import { resetProviders } from '@/hooks/useProviders';
import type { ProviderStatus } from '@/model/types';
import OnboardingGate, { exempt, gateOf } from './gate';

const MODEL: ProviderStatus = { id: 'anthropic', kind: 'anthropic', label: 'Anthropic', available: true, checked: true } as ProviderStatus;
const ALL = { worlds: 1, scenarios: 1, setups: 1, experiments: 0 };
const EMPTY = { worlds: 0, scenarios: 0, setups: 0, experiments: 0 };
const open = (counts: Record<string, number>) => ({ open: true as const, status: { name: 'P', counts, problems: [] } as never });

function mockFetch(project: { status: number; body: unknown }, providers: unknown[]) {
  vi.stubGlobal('fetch', vi.fn(async (url: string) => {
    const u = String(url);
    if (u.endsWith('/api/project')) return new Response(JSON.stringify(project.body), { status: project.status });
    if (u.endsWith('/api/providers')) return new Response(JSON.stringify(providers));
    return new Response(JSON.stringify({ error: 'not found' }), { status: 404 });
  }));
}

const show = (at: string) => render(
  <MemoryRouter initialEntries={[at]}>
    <OnboardingGate><Routes><Route path="/get-started" element={<p>get started</p>} /><Route path="*" element={<p>the app</p>} /></Routes></OnboardingGate>
  </MemoryRouter>,
);

beforeEach(() => { forgetShared(); resetProviders(); });
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe('onboarding gate', () => {
  it('gate_from_server_state', () => {
    expect(gateOf(null, null, false)).toBe('wait');
    expect(gateOf(open(ALL), [MODEL], false)).toBe('open');
    expect(gateOf(open(EMPTY), [MODEL], false)).toBe('open');
    expect(gateOf(open(ALL), [{ ...MODEL, available: false }], false)).toBe('shut');
    expect(gateOf({ open: false, message: 'no project', folder: true }, [MODEL], false)).toBe('shut');
    expect(gateOf(null, null, true)).toBe('open');
  });

  it('exempt_pages', () => {
    expect(exempt('/get-started')).toBe(true);
    for (const p of ['/', '/experiments', '/worlds/mill', '/settings/models', '/runs', '/analytics']) expect(exempt(p)).toBe(false);
  });

  it('no_model_sends_to_get_started', async () => {
    mockFetch({ status: 200, body: { name: 'P', counts: ALL, problems: [] } }, [{ ...MODEL, available: false }]);
    show('/experiments');
    expect(await screen.findByText('get started')).toBeTruthy();
  });

  it('no_project_sends_to_get_started', async () => {
    mockFetch({ status: 409, body: { error: 'no project is open', folder: false } }, [MODEL]);
    show('/');
    expect(await screen.findByText('get started')).toBeTruthy();
  });

  it('a_project_and_a_model_open_the_app_with_nothing_else', async () => {
    mockFetch({ status: 200, body: { name: 'P', counts: EMPTY, problems: [] } }, [MODEL]);
    show('/experiments');
    expect(await screen.findByText('the app')).toBeTruthy();
  });

  it('get_started_stays_while_unfinished', () => {
    mockFetch({ status: 409, body: { error: 'no project is open', folder: true } }, []);
    show('/get-started');
    expect(screen.getByText('get started')).toBeTruthy();
  });
});
