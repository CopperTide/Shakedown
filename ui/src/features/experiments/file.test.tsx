// @vitest-environment jsdom
// A file experiment's page: its scenario, setups and repeats, its problems, and a clear page for
// an id that doesn't exist.
import { cleanup, render, screen } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';
import FileExperimentPage from './FileExperimentPage';

const EXPERIMENT = {
  kind: 'experiment', id: 'slip', label: 'Supplier slip: solo vs team', hash: 'h', problems: [] as unknown[],
  body: { format: 'shakedown-experiment/1', label: 'Supplier slip: solo vs team', world: 'larkspur-plant', scenario: 'supplier-slip',
    setups: ['solo', 'team', 'rules'], repeats: { per_setup: 5, paired_seeds: true }, budget: { per_run_usd: 1.5 } },
};

function mockFetch(experiment: unknown) {
  vi.stubGlobal('fetch', vi.fn(async (url: string) => {
    const u = String(url);
    if (u.endsWith('/api/project/experiments/slip') && experiment) return new Response(JSON.stringify(experiment));
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
  it('shows_setups_and_repeats', async () => {
    mockFetch(EXPERIMENT);
    show('slip');
    expect(await screen.findByText('15 runs')).toBeTruthy();
    expect(await screen.findByText('Label of team')).toBeTruthy();
    expect(screen.getByText('Label of supplier-slip')).toBeTruthy();
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

  it('deploy_waits_for_the_runtime', async () => {
    mockFetch(EXPERIMENT);
    show('slip');
    const deploy = await screen.findByRole('button', { name: /Deploy/ });
    expect((deploy as HTMLButtonElement).disabled).toBe(true);
  });
});
