// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import type { SeriesPoint, Summary } from '@/api/analytics';
import MoreCharts from './MoreCharts';
import UsageChart from './UsageChart';
import { durationLabel, usageBuckets } from './moreChartsData';

const point = (extra: Partial<SeriesPoint>): SeriesPoint => ({ t: '2026-11-24T00:00:00Z', group: 'a', judged: 1, passed: 0, pass_rate: 0, runs: 1, cost_usd: 0, uncosted: 0, tokens: 100, ...extra });
const summary = (extra: Partial<Summary> = {}): Summary => ({
  group: 'setup', bucket: 'day', totals: { runs: 0, judged: 0, passed: 0, pass_rate: null, cost_usd: 0, cost_per_run: null, cost_per_pass: null, median_seconds: null, tokens: 0, uncosted: 0 },
  pending: { unprocessed: 0, running: 0 }, series: [], points: [], leaderboard: [], options: { world: [], scenario: [], setup: [], model: [], status: [] },
  failures: { checks: [], gates: [], reasons: [] }, duration: [], ...extra,
});
class NoResize { observe() {} unobserve() {} disconnect() {} }
globalThis.ResizeObserver ??= NoResize as unknown as typeof ResizeObserver;
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

it('aggregates groups without converting unknown costs into zero', () => {
  const rows = usageBuckets([point({ group: 'b', cost_usd: null, uncosted: 1 }), point({ group: 'a', cost_usd: 2.5 }), point({ t: '2026-11-23T00:00:00Z', cost_usd: null, uncosted: 1 })]);
  expect(rows.map((r) => r.cost_usd)).toEqual([null, 2.5]);
  expect(rows[1]).toMatchObject({ runs: 2, tokens: 200, uncosted: 1 });
  expect(usageBuckets([point({})])[0].cost_usd).toBe(0);
});
it('labels non-overlapping duration ranges with explicit boundaries', () => {
  expect(durationLabel({ lower_seconds: 0, upper_seconds: 10, runs: 1 })).toBe('0 s to < 10 s');
  expect(durationLabel({ lower_seconds: 60, upper_seconds: 300, runs: 1 })).toBe('1 min to < 5 min');
  expect(durationLabel({ lower_seconds: 3600, upper_seconds: null, runs: 1 })).toBe('1 h or more');
});
it('renders meaningful empty states', () => {
  render(<MoreCharts summary={summary()} />);
  expect(screen.getAllByText('No finished runs in this time range.')).toHaveLength(2);
  expect(screen.getByText('No named check failures recorded for these runs.')).toBeTruthy();
  expect(screen.getByText('No judged run durations available.')).toBeTruthy();
});
it('exposes exact counts and time data to keyboard and screen-reader users', () => {
  render(<MoreCharts summary={summary({
    series: [point({}), point({ t: '2026-11-25T00:00:00Z', cost_usd: null, uncosted: 1 })],
    failures: { checks: [{ name: 'legal schedule', runs: 3 }], gates: [{ name: 'release', runs: 2 }], reasons: [{ name: 'Failed', runs: 4 }] },
    duration: [{ lower_seconds: 0, upper_seconds: 10, runs: 3 }, { lower_seconds: 3600, upper_seconds: null, runs: 1 }],
  })} />);
  const checks = within(screen.getByRole('table', { name: 'Runs with failing checks' }));
  expect(checks.getByRole('row', { name: 'legal schedule 3' })).toBeTruthy();
  expect(screen.getByText(/Cost unavailable for 1 run/)).toBeTruthy();
  fireEvent.click(screen.getByText('View cost data'));
  const costs = within(screen.getByRole('table', { name: 'Cost data by day' }));
  expect(costs.getByText('$0.00')).toBeTruthy();
  expect(costs.getByText('Unknown')).toBeTruthy();
  expect(screen.getByRole('table', { name: 'Run-time distribution counts' })).toBeTruthy();
});

it('keeps pixel-size labels and a fixed chart height on narrow cards', () => {
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(300);
  const rows = usageBuckets([point({ t: '2026-11-23T00:00:00Z' }), point({ t: '2026-11-24T00:00:00Z' }), point({ t: '2026-11-25T00:00:00Z' })]);
  render(<UsageChart rows={rows} metric="tokens" bucket="day" />);
  const plot = screen.getByRole('img', { name: /Tokens per day/ });
  expect(plot.getAttribute('width')).toBe('300');
  expect(plot.getAttribute('height')).toBe('220');
  const ticks = [...plot.querySelectorAll('text')].filter((t) => t.getAttribute('y') === '202');
  expect(ticks).toHaveLength(2);
  expect(ticks.every((t) => t.getAttribute('font-size') === '11')).toBe(true);
});
