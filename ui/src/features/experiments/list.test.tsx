// @vitest-environment jsdom
// The experiments list: file experiments with sub-experiments under their parent, problems shown,
// search, and the earlier (database) experiments below.
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';
import ExperimentsListPage from './ExperimentsListPage';
import { type Row, filterExperiments, ordered } from './list';

const row = (id: string, extra: Partial<Row> = {}): Row => ({ id, label: id, scenario: 'supplier-slip', setups: 2, problems: [], ...extra });

function mockFetch(items: Record<string, unknown>[], db: unknown[] = []) {
  vi.stubGlobal('fetch', vi.fn(async (url: string) => {
    const u = String(url);
    if (u.endsWith('/api/project/experiments')) return new Response(JSON.stringify({ items }));
    const one = items.find((i) => u.endsWith(`/api/project/experiments/${i.id}`));
    if (one) return new Response(JSON.stringify(one));
    if (u.endsWith('/api/experiments')) return new Response(JSON.stringify(db));
    return new Response(JSON.stringify({ error: 'not found' }), { status: 404 });
  }));
}

afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe('experiments list', () => {
  it('children_follow_parent', () => {
    const rows = ordered([row('b', { parent: 'a' }), row('c'), row('a')]);
    expect(rows.map((r) => [r.id, r.depth])).toEqual([['a', 0], ['b', 1], ['c', 0]]);
  });

  it('search_filters_by_label_and_scenario', () => {
    const rows = [row('alpha'), row('beta', { scenario: 'rush-order' })];
    expect(filterExperiments(rows, { query: 'rush', problemsOnly: false }).map((r) => r.id)).toEqual(['beta']);
    expect(filterExperiments(rows, { query: 'ALP', problemsOnly: false }).map((r) => r.id)).toEqual(['alpha']);
    expect(filterExperiments([row('x', { problems: [{ pointer: '/world', message: 'no world' }] }), row('y')], { query: '', problemsOnly: true }).map((r) => r.id)).toEqual(['x']);
  });

  it('lists_problem_badge', async () => {
    const problem = { pointer: '/world', message: 'no world named gone' };
    mockFetch([{ id: 'exp', label: 'Supplier slip', hash: 'h', problems: [problem], kind: 'experiment',
      body: { scenario: 'supplier-slip', setups: ['a'], world: 'gone', repeats: { per_setup: 1 } } }]);
    render(<MemoryRouter><ExperimentsListPage /></MemoryRouter>);
    const badge = await screen.findByText('1 problem');
    expect(badge.getAttribute('title')).toBe('no world named gone');
    expect(screen.getByRole('link', { name: 'Supplier slip' }).getAttribute('href')).toBe('/experiments/file/exp');
  });

  it('empty_project_shows_new_experiment', async () => {
    mockFetch([]);
    render(<MemoryRouter><ExperimentsListPage /></MemoryRouter>);
    await waitFor(() => expect(screen.getByText('No experiments yet.')).toBeTruthy());
  });

  it('earlier_experiments_listed', async () => {
    mockFetch([], [{ id: 'db1', name: 'Old run set', tasks: [], benchmark: 'factory', runs: 3, created_at: '2026-11-01T00:00:00Z', status: 'finished', legacy: false, setups: [] }]);
    render(<MemoryRouter><ExperimentsListPage /></MemoryRouter>);
    const link = await screen.findByRole('link', { name: /^Old run set/ });
    expect(link.getAttribute('href')).toBe('/experiments/db1');
    expect(link.textContent).toContain('3 runs per setup');
    expect(link.className).toContain('hover:bg-hover');
  });
});

describe('moving earlier experiments', () => {
  it('move_into_project_confirms', async () => {
    const posts: unknown[] = [];
    const db = [{ id: 'db1', name: 'Old run set', tasks: [], benchmark: 'toybench', runs: 3, created_at: '2026-11-01T00:00:00Z', status: 'finished', legacy: false, setups: [] }];
    vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
      const u = String(url);
      if (u.endsWith('/api/project/import')) {
        const body = JSON.parse(String(init?.body));
        posts.push(body);
        return new Response(JSON.stringify(body.confirm ? { written: [], linked: 1, skipped: [] } : { proposed: true, files: [{ kind: 'world', id: 'toybench', label: 'Toy bench' }], skipped: ['Crew: Your code setups need their command'] }));
      }
      if (u.endsWith('/api/project/experiments')) return new Response(JSON.stringify({ items: [] }));
      if (u.endsWith('/api/experiments')) return new Response(JSON.stringify(db));
      return new Response(JSON.stringify({ error: 'not found' }), { status: 404 });
    }));
    render(<MemoryRouter><ExperimentsListPage /></MemoryRouter>);
    fireEvent.click(await screen.findByRole('button', { name: 'Move into the project' }));
    expect(await screen.findByText(/Toy bench/)).toBeTruthy();
    expect(screen.getByText(/Your code setups need their command/)).toBeTruthy();
    expect(posts).toEqual([{ confirm: false }]);
    fireEvent.click(screen.getByRole('button', { name: 'Move them' }));
    await waitFor(() => expect(posts).toEqual([{ confirm: false }, { confirm: true }]));
  });
});
