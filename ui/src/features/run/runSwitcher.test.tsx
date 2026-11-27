// @vitest-environment jsdom
// A run's switcher: finding its experiment, hopping to the next run, and ticking runs to compare.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { Experiment, ListedExperiment } from '@/model/experiment';
import type { Branch, BranchDetail } from '@/model/types';
import { candidates, switchOf } from './experimentOf';
import RunSwitcher from './RunSwitcher';

afterEach(() => { cleanup(); vi.unstubAllGlobals(); localStorage.clear(); });

const listed = (id: string, created_at: string, tasks = ['27'], legacy = false) => ({ id, created_at, tasks, legacy }) as ListedExperiment;
const runRow = (run: string | null, status = 'passed') => ({ task_id: '27', repeat: 0, run_id: run, branch_id: run && `${run}-b`, status, error: null,
  outcome: run ? { status, scored: true, checks_passed: 1, checks_total: 1, safety_violations: 0, denials: 0, flags: 0 } : null });
const setup = (name: string, runs: ReturnType<typeof runRow>[]) => ({ id: name, name, team: { id: 't', name: 't', agents: [] }, runs, checks: [],
  summary: { runs: runs.length, ended: 1, scored: 1, passed: 1, pass_rate: 1, unscored: 0, safety_violations: 0, denials: 0, flags: 0,
    checks_passed: {}, measured_s: {}, cost_usd: {} } });
const EXP = { id: 'e1', name: 'Solo vs pair', created_at: '2026-11-01T00:00:00Z', tasks: ['27'], legacy: false, status: 'finished',
  setups: [setup('Solo', [runRow('r1'), runRow('r2'), runRow(null, 'pending')]), setup('Pair', [runRow('r3', 'failed')])] } as unknown as Experiment;

describe('finding the experiment', () => {
  it('looks_at_experiments_of_the_task_that_started_before_the_run_newest_first', () => {
    const list = [listed('old', '2026-10-01T00:00:00Z'), listed('new', '2026-11-01T00:00:00Z'), listed('later', '2026-12-01T00:00:00Z'),
      listed('other', '2026-11-01T00:00:00Z', ['9']), listed('legacy', '2026-11-01T00:00:00Z', ['27'], true)];
    expect(candidates(list, '27', '2026-11-15T00:00:00Z').map((e) => e.id)).toEqual(['new', 'old']);
  });

  it('places_the_run_among_the_started_runs', () => {
    const s = switchOf(EXP, 'r2');
    expect(s.started.map((r) => [r.setup, r.n, r.of])).toEqual([['Solo', 1, 3], ['Solo', 2, 3], ['Pair', 1, 1]]);
    expect(s.at).toBe(1);
  });
});

function Where() { const l = useLocation(); return <output>{l.pathname + l.search}</output>; }

const detail = { id: 'r2-b', run_id: 'r2', created_at: '2026-11-02T00:00:00Z', status: 'passed', events: [
  { branch_id: 'r2-b', seq: 1, at: '2026-11-02T00:00:00Z', agent: null, kind: 'run_start', payload: { task_id: '27' } }] } as unknown as BranchDetail;

function show(siblings: Branch[] = []) {
  vi.stubGlobal('fetch', vi.fn(async (url: string) => {
    const u = String(url);
    if (u.endsWith('/api/experiments')) return new Response(JSON.stringify([listed('e1', EXP.created_at)]));
    if (u.endsWith('/api/experiments/e1')) return new Response(JSON.stringify(EXP));
    return new Response('{}', { status: 404 });
  }));
  render(<TooltipProvider><MemoryRouter initialEntries={['/runs/r2/r2-b']}><Routes>
    <Route path="/runs/r2/r2-b" element={<RunSwitcher runId="r2" detail={detail} siblings={siblings} />} />
    <Route path="*" element={<Where />} />
  </Routes></MemoryRouter></TooltipProvider>);
}

describe('run switcher', () => {
  it('hops_to_the_next_run_of_the_experiment', async () => {
    show();
    expect(await screen.findByText('Solo · run 2 of 3')).toBeTruthy();
    expect(screen.getByRole('link', { name: 'Previous run' }).getAttribute('href')).toBe('/runs/r1/r1-b');
    fireEvent.click(screen.getByRole('link', { name: 'Next run' }));
    expect(screen.getByRole('status').textContent).toBe('/runs/r3/r3-b');
  });

  it('ticks_three_runs_and_compares_them', async () => {
    show();
    fireEvent.click(await screen.findByRole('button', { name: /Experiment · Solo vs pair/ }));
    fireEvent.click(screen.getByRole('button', { name: 'Pick runs to compare' }));
    expect(screen.getByText('1 ticked')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: /^Run 1 · Passed/ }));
    fireEvent.click(screen.getByRole('button', { name: /^Run 1 · Failed/ }));
    fireEvent.click(screen.getByRole('button', { name: 'Compare 3' }));
    expect(screen.getByRole('status').textContent).toBe('/compare?branches=r2-b,r1-b,r3-b');
  });

  it('lists_this_runs_reruns', async () => {
    const branch = (id: string, parent_id: string | null, fork_note: string | null) => ({ id, run_id: 'r2', parent_id, fork_note, status: 'passed', created_at: id }) as unknown as Branch;
    show([branch('r2-b', null, null), branch('r2-c', 'r2-b', 'Pricier model')]);
    fireEvent.click(await screen.findByRole('button', { name: /Experiment ·/ }));
    expect(screen.getByRole('link', { name: /^↳ Pricier model/ }).getAttribute('href')).toBe('/runs/r2/r2-c');
    expect(screen.getByRole('link', { name: /^Original/ }).className).toContain('border-fg');
  });

  it('shows_nothing_outside_an_experiment_without_reruns', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('[]')));
    const { container } = render(<MemoryRouter><RunSwitcher runId="r9" detail={detail} siblings={[]} /></MemoryRouter>);
    await new Promise((r) => setTimeout(r, 20));
    expect(container.textContent).toBe('');
  });
});
