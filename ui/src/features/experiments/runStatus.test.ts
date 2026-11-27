// Whether a file experiment ran, from its deployments; sorting and filtering the list by it.
import { describe, expect, it } from 'vitest';
import type { Experiment } from '@/model/experiment';
import { type Row, arrange, filterExperiments } from './list';
import { NOT_RUN, runStatusOf, runStatusTitle } from './runStatus';

const deployment = (status: Experiment['status'], ended: number, created_at = '2026-11-25T10:00:00Z') =>
  ({ id: 'd', status, created_at, setups: [{ summary: { ended } }] }) as unknown as Experiment;

const row = (id: string, extra: Partial<Row> = {}): Row => ({ id, label: id, scenario: 's', setups: 1, problems: [], ...extra });

describe('run status', () => {
  it('no_deployment_is_not_run', () => {
    expect(runStatusOf([])).toEqual(NOT_RUN);
    expect(runStatusTitle(NOT_RUN)).toBe('Set up, never deployed');
  });

  it('ended_runs_are_results', () => {
    const s = runStatusOf([deployment('finished', 2, '2026-11-26T10:00:00Z'), deployment('stopped', 1)]);
    expect(s).toEqual({ status: 'results', ended: 3, deployments: 2, last: '2026-11-26T10:00:00Z' });
    expect(runStatusTitle(s)).toMatch(/^3 runs ended over 2 deployments · last /);
  });

  it('a_running_deployment_wins', () => {
    expect(runStatusOf([deployment('running', 0), deployment('finished', 4)]).status).toBe('running');
  });

  it('deployed_with_nothing_ended_has_no_results', () => {
    expect(runStatusOf([deployment('stopped', 0)]).status).toBe('no_results');
  });
});

describe('list by status', () => {
  const rows = [
    row('a', { run: NOT_RUN }),
    row('b', { run: { status: 'results', ended: 1, deployments: 1, last: '2026-11-20T00:00:00Z' } }),
    row('c', { run: { status: 'running', ended: 0, deployments: 1, last: '2026-11-26T00:00:00Z' }, parent: 'a' }),
    row('d'),
  ];

  it('status_sort_puts_results_first_and_flattens', () => {
    expect(arrange(rows, 'status').map((r) => [r.id, r.depth])).toEqual([['b', 0], ['c', 0], ['a', 0], ['d', 0]]);
  });

  it('recent_sort_puts_the_newest_deployment_first', () => {
    expect(arrange(rows, 'recent').map((r) => r.id)).toEqual(['c', 'b', 'a', 'd']);
  });

  it('tree_sort_keeps_sub_experiments_under_their_parent', () => {
    expect(arrange(rows, 'tree').map((r) => [r.id, r.depth])).toEqual([['a', 0], ['c', 1], ['b', 0], ['d', 0]]);
  });

  it('filters_by_status', () => {
    expect(filterExperiments(rows, { query: '', problemsOnly: false, status: 'not_run' }).map((r) => r.id)).toEqual(['a']);
    expect(filterExperiments(rows, { query: '', problemsOnly: false, status: 'all' })).toHaveLength(4);
  });
});
