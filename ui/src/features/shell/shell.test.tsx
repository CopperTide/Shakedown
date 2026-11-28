// @vitest-environment jsdom
// The shell: the sidebar's sections with the current one marked, the project's name, Ask, and the
// old addresses still answering.
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import App from '@/App';
import { forgetShared } from '@/hooks/query';
import { resetProviders } from '@/hooks/useProviders';
import Sidebar from './Sidebar';
import TopBar, { ASK_EVENT } from './TopBar';

const fetched: string[] = [];

function mockFetch(answers: Record<string, unknown>) {
  vi.stubGlobal('fetch', vi.fn(async (url: string) => {
    fetched.push(String(url));
    const key = Object.keys(answers).find((k) => String(url).endsWith(k));
    if (!key) return new Response(JSON.stringify({ error: 'not found' }), { status: 404 });
    const body = answers[key];
    return new Response(JSON.stringify(body ?? { error: 'no project is open' }), { status: body === null ? 409 : 200 });
  }));
}

beforeEach(() => { fetched.length = 0; forgetShared(); resetProviders(); });
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe('shell', () => {
  it('sidebar_groups_and_active', () => {
    mockFetch({ '/api/project': { name: 'Larkspur Ops', counts: {}, problems: [] } });
    render(<MemoryRouter initialEntries={['/scenarios']}><Sidebar /></MemoryRouter>);
    expect(screen.getByText('Build')).toBeTruthy();
    expect(screen.getByRole('link', { name: 'Scenarios' }).getAttribute('aria-current')).toBe('page');
    expect(screen.getByRole('link', { name: 'Home' }).getAttribute('aria-current')).toBeNull();
  });


  it('sidebar_shows_only_the_core', () => {
    mockFetch({ '/api/project': { name: 'Larkspur Ops', counts: {}, problems: [] } });
    render(<MemoryRouter><Sidebar /></MemoryRouter>);
    const links = screen.getAllByRole('link').map((a) => a.textContent).filter((t) => t !== 'Shakedown');
    expect(links).toEqual(['Home', 'Experiments', 'Worlds', 'Scenarios', 'Agent setups', 'Get started', 'Settings']);
  });

  it.each(['/settings/observability', '/settings/tokens', '/runtimes', '/connect'])('marks Settings on %s', (at) => {
    mockFetch({ '/api/project': { name: 'Larkspur Ops', counts: {}, problems: [] } });
    render(<MemoryRouter initialEntries={[at]}><Sidebar /></MemoryRouter>);
    expect(screen.getByRole('link', { name: 'Settings' }).getAttribute('aria-current')).toBe('page');
  });

  it('shows_project_name', async () => {
    mockFetch({ '/api/project': { name: 'Larkspur Ops', counts: {}, problems: [] } });
    render(<MemoryRouter><Sidebar /></MemoryRouter>);
    await waitFor(() => expect(screen.getByText('Larkspur Ops')).toBeTruthy());
  });

  it('ask_dispatches_event', () => {
    mockFetch({});
    const heard = vi.fn();
    window.addEventListener(ASK_EVENT, heard);
    render(<MemoryRouter><TopBar onPalette={() => {}} /></MemoryRouter>);
    fireEvent.click(screen.getByRole('button', { name: /Ask/ }));
    expect(heard).toHaveBeenCalled();
    window.removeEventListener(ASK_EVENT, heard);
  });

  it('old_routes_still_resolve', async () => {
    const counts = { worlds: 1, scenarios: 1, setups: 1, experiments: 0 };
    mockFetch({ '/api/project': { name: 'P', counts, problems: [] }, '/api/providers': [{ id: 'anthropic', available: true }] });
    render(<MemoryRouter initialEntries={['/experiments/abc']}><App /></MemoryRouter>);
    await waitFor(() => expect(fetched.some((u) => u.endsWith('/api/experiments/abc'))).toBe(true));
  });

  it('analytics_opens_experiments', async () => {
    const counts = { worlds: 1, scenarios: 1, setups: 1, experiments: 0 };
    mockFetch({ '/api/project': { name: 'P', counts, problems: [] }, '/api/providers': [{ id: 'anthropic', available: true }] });
    render(<MemoryRouter initialEntries={['/analytics']}><App /></MemoryRouter>);
    await waitFor(() => expect(screen.getByRole('heading', { level: 1, name: 'Experiments' })).toBeTruthy());
  });
});
