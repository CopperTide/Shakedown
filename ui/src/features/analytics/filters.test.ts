// The Analytics filter in the address, the API query it makes, and the drill-down into the runs list.
import { describe, expect, it } from 'vitest';
import { filterRuns, parseRunFilter } from '@/features/runs/runFilter';
import type { RunSummary } from '@/model/types';
import { apiQuery, fromOf, parseFilter, runsHref, writeFilter } from './filters';

const NOW = new Date('2026-11-25T15:30:00Z');
const TEAM = 'OCC team (duty manager, four controllers)';

describe('the analytics filter', () => {
  it('round-trips through the address, names with commas included, defaults left out', () => {
    const f = { ...parseFilter(new URLSearchParams()), setup: [TEAM, 'Solo'], range: '7d' as const, group: 'model' as const };
    const sp = writeFilter(new URLSearchParams('tab=x'), f);
    expect(sp.getAll('setup')).toEqual([TEAM, 'Solo']);
    expect(sp.get('tab')).toBe('x');
    expect(parseFilter(sp)).toEqual(f);
    expect(writeFilter(new URLSearchParams(), parseFilter(new URLSearchParams())).toString()).toBe('');
    expect(parseFilter(new URLSearchParams('range=1y&group=colour'))).toMatchObject({ range: '30d', group: 'setup' });
  });

  it('starts a range at midnight UTC so it holds all day, and all time has no start', () => {
    expect(fromOf('7d', NOW)).toBe('2026-11-19T00:00:00.000Z');
    expect(fromOf('30d', NOW)).toBe('2026-10-27T00:00:00.000Z');
    expect(fromOf('all', NOW)).toBeNull();
  });

  it('asks the API with each list as a JSON array', () => {
    const q = new URLSearchParams(apiQuery({ ...parseFilter(new URLSearchParams()), setup: [TEAM] }, NOW).slice(1));
    expect(JSON.parse(q.get('setup')!)).toEqual([TEAM]);
    expect(q.get('from')).toBe('2026-10-27T00:00:00.000Z');
    expect(q.get('group')).toBe('setup');
    expect(q.has('world')).toBe(false);
  });

  it('drills down to the runs list with the mark’s setup or model, the range and a lone pick carried along', () => {
    const f = { ...parseFilter(new URLSearchParams('range=all')), scenario: ['refund'] };
    const href = new URL(runsHref(f, NOW, { model: 'm1' }), 'http://x');
    expect(href.pathname).toBe('/runs');
    expect(Object.fromEntries(href.searchParams)).toEqual({ scenario: 'refund', model: 'm1' });
    const week = new URL(runsHref(parseFilter(new URLSearchParams('range=7d')), NOW, { setup: TEAM, scenario: 's' }), 'http://x');
    expect(Object.fromEntries(week.searchParams)).toEqual({ from: '2026-11-19T00:00:00.000Z', setup: TEAM, scenario: 's' });
  });
});

const run = (id: string, team: string, task: string, at: string, model = 'm1'): RunSummary => ({
  id, task_id: task, created_at: at,
  branches: [{ id: `${id}b`, run_id: id, parent_id: null, start_seq: null, changes: null, fork_note: null, status: 'passed', error: null, created_at: at, ended_at: at,
    team: { id: 't', name: team, agents: [{ id: 'a', name: 'A', kind: 'open', model, prompt: '' }] } }],
});

describe('the runs list narrowed by a drill-down', () => {
  const runs = [run('1', TEAM, 'refund', '2026-11-20T10:00:00Z'), run('2', 'Solo', 'refund', '2026-11-24T10:00:00Z', 'm2'), run('3', TEAM, 'exchange', '2026-11-01T10:00:00Z')];
  const ids = (q: string) => filterRuns(runs, parseRunFilter(new URLSearchParams(q))).map((r) => r.id);
  it('matches the first branch’s setup and models, the scenario and the time', () => {
    expect(ids('')).toEqual(['1', '2', '3']);
    expect(ids(`setup=${encodeURIComponent(TEAM)}`)).toEqual(['1', '3']);
    expect(ids(`setup=${encodeURIComponent(TEAM)}&scenario=refund`)).toEqual(['1']);
    expect(ids('model=m2')).toEqual(['2']);
    expect(ids('from=2026-11-19T00:00:00Z')).toEqual(['1', '2']);
    expect(ids('to=2026-11-19T00:00:00Z')).toEqual(['3']);
  });
});
