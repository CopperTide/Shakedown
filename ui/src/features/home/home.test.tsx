// @vitest-environment jsdom
// Home: your information first. Without a project it points to Get started; with one it shows the
// week, what needs attention, and the project's contents.
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { forgetShared } from '@/hooks/query';
import type { RunSummary } from '@/model/types';
import { attention, weekStats } from './home';
import HomePage from './HomePage';

const NOW = Date.parse('2026-11-21T12:00:00Z');

function run(id: string, daysAgo: number, status: string): RunSummary {
  const at = new Date(NOW - daysAgo * 86_400_000).toISOString();
  return { id, task_id: 't', created_at: at, branches: [{ id: `${id}b`, run_id: id, parent_id: null, start_seq: null, team: { agents: [] } as never, changes: null, fork_note: null, status: status as never, error: null, created_at: at, ended_at: null }] };
}

function mockFetch(project: unknown, runs: RunSummary[] = []) {
  vi.stubGlobal('fetch', vi.fn(async (url: string) => {
    if (String(url).endsWith('/api/project')) {
      return project ? new Response(JSON.stringify(project)) : new Response(JSON.stringify({ error: 'no project is open' }), { status: 409 });
    }
    if (String(url).endsWith('/api/runs')) return new Response(JSON.stringify(runs));
    return new Response(JSON.stringify({ error: 'not found' }), { status: 404 });
  }));
}

beforeEach(() => forgetShared());
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe('home', () => {
  it('week_stats_counts_last_7_days', () => {
    const stats = weekStats([run('a', 1, 'passed'), run('b', 2, 'failed'), run('c', 9, 'passed'), run('d', 0, 'running')], NOW);
    expect(stats).toEqual({ runs: 3, passed: 1, failed: 1, running: 1, passRate: 0.5 });
  });

  it('attention_lists_failures_newest_first', () => {
    const list = attention([run('a', 3, 'failed'), run('b', 1, 'failed'), run('c', 0, 'passed')]);
    expect(list.map((b) => b.run_id)).toEqual(['b', 'a']);
  });

  it('errored_stopped_and_over_budget_runs_count_as_failed', () => {
    const runs = ['infra_error', 'stopped', 'interrupted', 'budget_exceeded', 'failed', 'passed', 'queued'].map((s, i) => run(s, i / 10, s));
    expect(weekStats(runs, NOW)).toEqual({ runs: 7, passed: 1, failed: 5, running: 0, passRate: 1 / 6 });
    expect(attention(runs).map((b) => b.run_id)).toEqual(['infra_error', 'stopped', 'interrupted', 'budget_exceeded', 'failed']);
  });

  it('home_without_project', async () => {
    mockFetch(null);
    render(<MemoryRouter><HomePage /></MemoryRouter>);
    await waitFor(() => expect(screen.getByText('Set up your project')).toBeTruthy());
    expect(screen.getByRole('link', { name: /Get started/ }).getAttribute('href')).toBe('/get-started');
  });

  it('home_with_project', async () => {
    mockFetch({ name: 'Larkspur Ops', counts: { worlds: 1, scenarios: 2, setups: 3, experiments: 1 }, problems: [] });
    render(<MemoryRouter><HomePage /></MemoryRouter>);
    await waitFor(() => expect(screen.getByText('Larkspur Ops')).toBeTruthy());
    expect(screen.getByRole('link', { name: /Agent setups/ }).textContent).toContain('3');
    expect(screen.getByRole('link', { name: /Worlds/ }).getAttribute('href')).toBe('/worlds');
  });
});
