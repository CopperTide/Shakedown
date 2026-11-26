// @vitest-environment jsdom
// The Analytics page: its filters live in the address and reach the API, it says plainly what to
// do when there are no runs, and its leaderboard rows open the matching runs.
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Summary } from '@/api/analytics';
import AnalyticsPage from './AnalyticsPage';

vi.mock('uplot', () => ({ default: class { height = 220; setSize() {} destroy() {} } }));
class NoResize { observe() {} unobserve() {} disconnect() {} }
globalThis.ResizeObserver ??= NoResize as unknown as typeof ResizeObserver;

const TEAM = 'OCC team (duty manager, four controllers)';
const EMPTY: Summary = {
  group: 'setup', bucket: 'day', pending: { unprocessed: 0, running: 0 }, series: [], points: [], leaderboard: [],
  options: { world: [], scenario: [], setup: [], model: [], status: [] },
  failures: { checks: [], gates: [], reasons: [] }, duration: [],
  totals: { runs: 0, judged: 0, passed: 0, pass_rate: null, cost_usd: 0, cost_per_run: null, cost_per_pass: null, median_seconds: null, tokens: 0, uncosted: 0 },
};
const SOME: Summary = {
  ...EMPTY,
  totals: { runs: 3, judged: 3, passed: 2, pass_rate: 2 / 3, cost_usd: 1.5, cost_per_run: 0.5, cost_per_pass: 0.75, median_seconds: 64, tokens: 450, uncosted: 0 },
  series: [{ t: '2026-11-24T00:00:00Z', group: TEAM, judged: 3, passed: 2, pass_rate: 2 / 3, runs: 3, cost_usd: 1.5, uncosted: 0, tokens: 450 }],
  points: [{ group: TEAM, runs: 3, judged: 3, passed: 2, pass_rate: 2 / 3, cost_usd: 1.5, cost_per_run: 0.5, tokens: 450 }],
  leaderboard: [{ setup: TEAM, world: 'airline-hub-day', scenario: 'recover-the-bank', runs: 3, judged: 3, passed: 2, pass_rate: 2 / 3, cost_per_run: 0.5, median_seconds: 64, cost_per_pass: 0.75 }],
  options: { world: ['airline-hub-day'], scenario: ['recover-the-bank'], setup: [TEAM], model: ['m1'], status: ['passed', 'failed'] },
};

const asked: URLSearchParams[] = [];
function serve(summary: Summary) {
  asked.length = 0;
  vi.stubGlobal('fetch', vi.fn(async (url: string) => {
    asked.push(new URL(String(url), 'http://x').searchParams);
    return new Response(JSON.stringify(summary));
  }));
}

function Where() {
  const l = useLocation();
  return <output data-testid="where">{l.pathname + l.search}</output>;
}

const show = (at = '/analytics') => render(
  <MemoryRouter initialEntries={[at]}>
    <Routes><Route path="/analytics" element={<AnalyticsPage />} /><Route path="*" element={null} /></Routes>
    <Where />
  </MemoryRouter>,
);
const where = () => new URL(screen.getByTestId('where').textContent!, 'http://x');

afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe('the Analytics page', () => {
  it('with no runs at all, says to deploy an experiment', async () => {
    serve(EMPTY);
    show('/analytics?range=all');
    expect(await screen.findByText('No finished runs yet')).toBeTruthy();
    expect(screen.getByText(/Deploy an experiment to see results here/)).toBeTruthy();
    expect(screen.getByRole('link', { name: 'Go to Experiments' }).getAttribute('href')).toBe('/experiments');
  });

  it('with no runs in a shorter range or under a filter, offers to widen it', async () => {
    serve(EMPTY);
    show();
    fireEvent.click(await screen.findByRole('button', { name: 'Show all time' }));
    await waitFor(() => expect(where().searchParams.get('range')).toBe('all'));
    cleanup();
    show(`/analytics?setup=${encodeURIComponent(TEAM)}`);
    expect(await screen.findByText('No runs match these filters')).toBeTruthy();
  });

  it('keeps the range, the picks and the grouping in the address, and asks the API for them', async () => {
    serve(SOME);
    show(`/analytics?setup=${encodeURIComponent(TEAM)}&setup=Solo`);
    await screen.findByText('Leaderboard');
    expect(JSON.parse(asked.at(-1)!.get('setup')!)).toEqual([TEAM, 'Solo']);
    expect(asked.at(-1)!.get('from')).toMatch(/T00:00:00.000Z$/);

    fireEvent.click(screen.getByRole('radio', { name: '7 d' }));
    await waitFor(() => expect(where().searchParams.get('range')).toBe('7d'));
    fireEvent.click(screen.getByRole('radio', { name: 'Model' }));
    await waitFor(() => expect(where().searchParams.get('group')).toBe('model'));
    await waitFor(() => expect(asked.at(-1)!.get('group')).toBe('model'));

    fireEvent.click(screen.getByRole('button', { name: 'Remove setup filter: Solo' }));
    await waitFor(() => expect(where().searchParams.getAll('setup')).toEqual([TEAM]));
    fireEvent.click(screen.getByRole('radio', { name: 'All' }));
    await waitFor(() => expect(asked.at(-1)!.has('from')).toBe(false));
  });

  it('shows the figures, and a leaderboard row opens its runs', async () => {
    serve(SOME);
    show('/analytics?range=all');
    expect(await screen.findByText('67%', { selector: 'span.font-display' })).toBeTruthy();
    expect(screen.getByText(/Only 3 runs so far/)).toBeTruthy();
    fireEvent.click(screen.getByText('recover-the-bank', { selector: 'td span' }));
    await waitFor(() => expect(where().pathname).toBe('/runs'));
    expect(Object.fromEntries(where().searchParams)).toEqual({ setup: TEAM, scenario: 'recover-the-bank' });
  });

  it('says so when the summary cannot be read', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ error: 'database down' }), { status: 500 })));
    show();
    expect(await screen.findByRole('alert')).toBeTruthy();
  });
});
