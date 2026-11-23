// @vitest-environment jsdom
// The shell: the sidebar's sections with the current one marked, the project's name, Ask, and the
// old addresses still answering.
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import App from '@/App';
import { forgetShared } from '@/hooks/query';
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

beforeEach(() => { fetched.length = 0; forgetShared(); });
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe('shell', () => {
  it('sidebar_groups_and_active', () => {
    mockFetch({ '/api/project': { name: 'Larkspur Ops', counts: {}, problems: [] } });
    render(<MemoryRouter initialEntries={['/scenarios']}><Sidebar /></MemoryRouter>);
    expect(screen.getByText('Build')).toBeTruthy();
    expect(screen.getByText('Settings')).toBeTruthy();
    expect(screen.getByRole('link', { name: 'Scenarios' }).getAttribute('aria-current')).toBe('page');
    expect(screen.getByRole('link', { name: 'Home' }).getAttribute('aria-current')).toBeNull();
  });

  it('sidebar_lists_worlds_under_their_groups', async () => {
    const world = (id: string, label: string, group?: string) => ({ kind: 'world', id, label, hash: 'h', problems: [], group: group ?? null });
    mockFetch({
      '/api/project': { name: 'Larkspur Ops', counts: {}, problems: [] },
      '/api/project/worlds': { items: [world('solo', 'Larkspur'), world('b', 'West plant', 'Rebolt'), world('a', 'East plant', 'Rebolt'), world('m', 'Mill', 'Arden')] },
    });
    render(<MemoryRouter initialEntries={['/worlds/a']}><Sidebar /></MemoryRouter>);
    const tree = await screen.findByRole('list', { name: 'Worlds by group' });
    expect(tree.textContent).toBe('ArdenReboltEast plantWest plantUngrouped');
    const east = screen.getByRole('link', { name: 'East plant' });
    expect(east.getAttribute('href')).toBe('/worlds/a');
    expect(east.getAttribute('aria-current')).toBe('page');
    fireEvent.click(screen.getByRole('button', { name: 'Arden' }));
    expect(screen.getByRole('link', { name: 'Mill' }).getAttribute('href')).toBe('/worlds/m');
    fireEvent.click(screen.getByRole('button', { name: 'Rebolt' }));
    expect(screen.queryByRole('link', { name: 'East plant' })).toBeNull();
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
    mockFetch({ '/api/project': null });
    render(<MemoryRouter initialEntries={['/experiments/abc']}><App /></MemoryRouter>);
    await waitFor(() => expect(fetched.some((u) => u.endsWith('/api/experiments/abc'))).toBe(true));
  });
});
