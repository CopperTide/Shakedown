// @vitest-environment jsdom
// Experiment settings and the guided new experiment: scenarios follow the chosen world, repeats
// save to the file, and Create writes a valid experiment and opens it.
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { scenariosOf } from './choices';
import ExperimentSettings from './ExperimentSettings';
import NewExperimentPage from './NewExperimentPage';

const EXP = { format: 'shakedown-experiment/1', label: 'Slip', world: 'plant', scenario: 'slip', setups: ['solo'], repeats: { per_setup: 1 } };
let puts: { url: string; body: Record<string, unknown> }[] = [];

function mockFetch() {
  puts = [];
  const items = (list: { id: string; label: string; world?: string }[]) => new Response(JSON.stringify({ items: list.map((i) => ({ ...i, problems: [] })) }));
  vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
    const u = String(url);
    if (init?.method === 'PUT') { const sent = JSON.parse(String(init.body)); puts.push({ url: u, body: sent.body }); return new Response(JSON.stringify({ kind: 'experiment', id: 'x', label: 'x', hash: 'h', problems: [], body: sent.body })); }
    if (u.endsWith('/api/project/worlds')) return items([{ id: 'plant', label: 'Plant' }]);
    if (u.endsWith('/api/project/setups')) return items([{ id: 'solo', label: 'Solo' }, { id: 'team', label: 'Team' }]);
    if (u.endsWith('/api/project/scenarios')) return items([{ id: 'slip', label: 'Slip', world: 'plant' }, { id: 'other', label: 'Other', world: 'elsewhere' }]);
    return new Response(JSON.stringify({ error: 'not found' }), { status: 404 });
  }));
}

afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe('experiment settings and new experiment', () => {
  it('scenario_choices_follow_world', () => {
    const scenarios = [{ id: 'a', label: 'A', world: 'plant' }, { id: 'b', label: 'B', world: 'other' }];
    expect(scenariosOf(scenarios, 'plant').map((s) => s.id)).toEqual(['a']);
  });

  it('settings_save_writes_repeats', async () => {
    mockFetch();
    const onSaved = vi.fn();
    render(<MemoryRouter><ExperimentSettings id="slip-exp" body={EXP} onSaved={onSaved} /></MemoryRouter>);
    fireEvent.change(await screen.findByLabelText('Runs per setup'), { target: { value: '5' } });
    fireEvent.click(await screen.findByRole('button', { name: 'Save' }));
    await waitFor(() => expect(puts.length).toBe(1));
    expect(puts[0].body.repeats).toEqual({ per_setup: 5 });
    expect(onSaved).toHaveBeenCalled();
  });

  it('new_flow_creates_file', async () => {
    mockFetch();
    render(<MemoryRouter initialEntries={['/experiments/new']}><Routes>
      <Route path="/experiments/new" element={<NewExperimentPage />} />
      <Route path="/experiments/file/:id" element={<p>opened</p>} />
    </Routes></MemoryRouter>);
    fireEvent.change(await screen.findByLabelText('Name'), { target: { value: 'Slip solo vs team' } });
    fireEvent.change(screen.getByLabelText('World'), { target: { value: 'plant' } });
    await waitFor(() => expect(screen.getByRole('option', { name: 'Slip' })).toBeTruthy());
    expect(screen.queryByRole('option', { name: 'Other' })).toBeNull();
    fireEvent.change(screen.getByLabelText('Scenario'), { target: { value: 'slip' } });
    fireEvent.click(screen.getByRole('checkbox', { name: 'Solo' }));
    fireEvent.click(screen.getByRole('checkbox', { name: 'Team' }));
    fireEvent.change(screen.getByLabelText('Runs per setup'), { target: { value: '3' } });
    fireEvent.change(screen.getByLabelText('Spend cap per run (USD)'), { target: { value: '0.5' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create experiment' }));
    expect(await screen.findByText('opened')).toBeTruthy();
    expect(puts[0].url).toMatch(/\/api\/project\/experiments\/slip-solo-vs-team$/);
    expect(puts[0].body).toEqual({ format: 'shakedown-experiment/1', label: 'Slip solo vs team', world: 'plant', scenario: 'slip', setups: ['solo', 'team'], repeats: { per_setup: 3, paired_seeds: true }, budget: { per_run_usd: 0.5 } });
  });

  it('settings_save_writes_the_run_cap_and_refuse_a_bad_one', async () => {
    mockFetch();
    render(<MemoryRouter><ExperimentSettings id="slip-exp" body={EXP} onSaved={() => {}} /></MemoryRouter>);
    const cap = await screen.findByLabelText('Spend cap per run (USD)');
    const save = screen.getByRole('button', { name: 'Save' }) as HTMLButtonElement;
    for (const bad of ['0', '-1', 'abc']) {
      fireEvent.change(cap, { target: { value: bad } });
      expect(save.disabled).toBe(true);
      expect(screen.getByText(/positive number of dollars/)).toBeTruthy();
    }
    fireEvent.change(cap, { target: { value: '0.25' } });
    fireEvent.click(save);
    await waitFor(() => expect(puts.length).toBe(1));
    expect(puts[0].body.budget).toEqual({ per_run_usd: 0.25 });
  });

  it('an_emptied_cap_leaves_no_budget', async () => {
    mockFetch();
    render(<MemoryRouter><ExperimentSettings id="slip-exp" body={{ ...EXP, budget: { per_run_usd: 2 } }} onSaved={() => {}} /></MemoryRouter>);
    const cap = await screen.findByLabelText('Spend cap per run (USD)') as HTMLInputElement;
    expect(cap.value).toBe('2');
    fireEvent.change(cap, { target: { value: '' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(puts.length).toBe(1));
    expect(puts[0].body).not.toHaveProperty('budget');
    fireEvent.change(cap, { target: { value: 'x' } });
    fireEvent.click(screen.getByRole('button', { name: 'Discard' }));
    expect(cap.value).toBe('2');
  });
});
