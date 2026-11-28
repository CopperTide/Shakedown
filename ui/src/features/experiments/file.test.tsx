// @vitest-environment jsdom
// A file experiment's page: its runs first, its configuration folded under them, its problems, and
// a clear page for an id that doesn't exist.
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';
import FileExperimentPage from './FileExperimentPage';

const EXPERIMENT = {
  kind: 'experiment', id: 'slip', label: 'Supplier slip: solo vs team', hash: 'h', problems: [] as unknown[],
  body: { format: 'shakedown-experiment/1', label: 'Supplier slip: solo vs team', world: 'larkspur-plant', scenario: 'supplier-slip',
    setups: ['solo', 'team', 'rules'], repeats: { per_setup: 5, paired_seeds: true }, budget: { per_run_usd: 1.5 } },
};

const posted: string[] = [];

function mockFetch(experiment: unknown, deploy: { status: number; body: unknown } = { status: 200, body: { id: 'db9' } }, deployments: unknown[] = []) {
  posted.length = 0;
  vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
    const u = String(url);
    if (u.endsWith('/deploy') && init?.method === 'POST') { posted.push(u); return new Response(JSON.stringify(deploy.body), { status: deploy.status }); }
    if (u.endsWith('/api/project/experiments/slip/runs')) return new Response(JSON.stringify({ deployments }));
    if (u.endsWith('/api/project/experiments/slip') && experiment) return new Response(JSON.stringify(experiment));
    const list = u.match(/\/api\/project\/(worlds|scenarios|setups)$/);
    if (list) return new Response(JSON.stringify({ items: list[1] === 'setups' ? ['solo', 'team', 'rules'].map((id) => ({ id, label: `Label of ${id}` })) : [] }));
    const label = (id: string) => new Response(JSON.stringify({ id, label: `Label of ${id}`, problems: [], body: {} }));
    for (const kind of ['scenarios', 'worlds', 'setups']) {
      const m = u.match(new RegExp(`/api/project/${kind}/([a-z-]+)$`));
      if (m) return label(m[1]);
    }
    return new Response(JSON.stringify({ error: 'no experiment named nope' }), { status: 404 });
  }));
}

const show = (id: string) => render(
  <MemoryRouter initialEntries={[`/experiments/file/${id}`]}>
    <Routes><Route path="/experiments/file/:id" element={<FileExperimentPage />} /></Routes>
  </MemoryRouter>,
);

afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe('file experiment page', () => {
  it('runs_first_configuration_folded', async () => {
    mockFetch(EXPERIMENT);
    show('slip');
    const line = await screen.findByText('3 setups × 5 runs = 15 runs · paired seeds · $1.50 cap a run');
    const config = line.closest('details')!;
    expect(config.open).toBe(false);
    expect(within(config).getByRole('checkbox', { name: 'Label of team' })).toBeTruthy();
    const runs = screen.getByRole('heading', { name: 'Runs' });
    expect(runs.compareDocumentPosition(config) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.getByText('Label of supplier-slip')).toBeTruthy();
  });

  it('breadcrumb_world_scenario_experiment', async () => {
    mockFetch(EXPERIMENT);
    show('slip');
    const trail = await screen.findByRole('navigation', { name: 'Where this experiment sits' });
    const links = within(trail).getAllByRole('link');
    expect(links.map((l) => [l.textContent, l.getAttribute('href')])).toEqual([
      ['Label of larkspur-plant', '/worlds/larkspur-plant'],
      ['Label of supplier-slip', '/scenarios/supplier-slip'],
    ]);
    expect(within(trail).getByText('Supplier slip: solo vs team').getAttribute('aria-current')).toBe('page');
  });

  it('offers_delete', async () => {
    mockFetch(EXPERIMENT);
    show('slip');
    expect(await screen.findByRole('button', { name: 'Delete' })).toBeTruthy();
  });

  it('unknown_file_experiment', async () => {
    mockFetch(null);
    show('nope');
    expect(await screen.findByText('No experiment named nope')).toBeTruthy();
    expect(screen.getByRole('link', { name: /Experiments/ }).getAttribute('href')).toBe('/experiments');
  });

  it('problems_listed', async () => {
    mockFetch({ ...EXPERIMENT, problems: [{ pointer: '/setups/2', message: 'no setup named rules' }] });
    show('slip');
    expect(await screen.findByText('no setup named rules')).toBeTruthy();
  });

  it('deploy_confirms_then_posts', async () => {
    mockFetch(EXPERIMENT);
    show('slip');
    fireEvent.click(await screen.findByRole('button', { name: /^Deploy$/ }));
    expect(await screen.findByRole('dialog', { name: 'Deploy 15 runs' })).toBeTruthy();
    expect(screen.getByText(/3 setups × 5 runs each/)).toBeTruthy();
    expect(posted).toEqual([]);
    fireEvent.click(screen.getByRole('button', { name: 'Deploy 15 runs' }));
    await waitFor(() => expect(posted.length).toBe(1));
    expect(posted[0]).toMatch(/\/api\/project\/experiments\/slip\/deploy$/);
  });

  it('deploy_error_shown', async () => {
    mockFetch(EXPERIMENT, { status: 422, body: { error: 'Larkspur Valve Works has no runtime yet' } });
    show('slip');
    fireEvent.click(await screen.findByRole('button', { name: /^Deploy$/ }));
    fireEvent.click(await screen.findByRole('button', { name: 'Deploy 15 runs' }));
    expect(await screen.findByText('Larkspur Valve Works has no runtime yet')).toBeTruthy();
  });

  it('lists_deployments', async () => {
    const deployment = { id: 'db1', name: 'Supplier slip', status: 'running', created_at: '2026-11-21T10:00:00Z', ended_at: null, error: null, runs: 5, tasks: ['t1'], benchmark: 'toybench', legacy: false,
      setups: [{ id: 's1', name: 'Solo planner', summary: { ended: 2, passed: 1 } }] };
    mockFetch(EXPERIMENT, undefined, [deployment]);
    show('slip');
    const link = await screen.findByRole('link', { name: /Open/ });
    expect(link.getAttribute('href')).toBe('/experiments/db1');
    expect(screen.getByText(/Solo planner/)).toBeTruthy();
  });
});
