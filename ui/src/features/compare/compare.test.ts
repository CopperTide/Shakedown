import { describe, expect, it } from 'vitest';
import { FORK_SEQ, comparison, v1Detail, v2Detail } from '@/features/world/fixtures';
import type { CompareSide, Event } from '@/model/types';
import { axisTicks, divergenceStrip } from './divergence';
import { type SideInput, agentCost, checksPassed, escalations, kpiFigure, lastView, leadingNumber, measureFigure, outcomeRows, safetyViolations, verdict } from './outcomes';

const ev = (seq: number, kind: Event['kind'], payload: unknown, agent: string | null = null): Event => ({ branch_id: 'b', seq, at: '', agent, kind, payload }) as Event;

describe('outcome comparison', () => {
  const rows = outcomeRows({ detail: v1Detail, side: comparison.a }, { detail: v2Detail, side: comparison.b });
  const row = (id: string) => rows.find((r) => r.id === id)!;

  it('reads every outcome of both branches', () => {
    expect(rows.map((r) => [r.label, r.a.text, r.b.text])).toEqual([
      ['Checks passed', '✕ 8/9', '✓ 9/9'],
      ['Safety violations', '◆ 1 of 4', '0 of 4'],
      ['Orders at risk', '4 of 10', '4 of 10'],
      ['Orders covered', '2 of 10', '4 of 10'],
      ['Safety violations so far', '1', '0'],
      ['Transport cost', 'USD 3,410', 'USD 4,660'],
      ['Agent cost', '$0.22 (reported) · 61,240 tokens', '$0.086 (reported) · 23,410 tokens'],
      ['Escalations', '3', '3'],
    ]);
    expect(row('checks').a).toMatchObject({ abnormal: true, title: 'Failed: eligible_stock_allocated' });
    expect(row('safety').a.title).toBe('no_short_dated_lot_shipped');
  });

  it('says which side did better, and nothing when equal, unknown or neither way is better', () => {
    expect(['checks', 'safety', 'escalations'].map((id) => verdict(row(id)))).toEqual(['b', 'b', null]);
    expect(verdict(row('agent_cost'))).toBe('b');
    expect(verdict(row('kpi:transport_cost')), 'a twin figure says no better way').toBeNull();
  });

  it('falls back when the run was not scored or drew no twin', () => {
    expect(checksPassed([])).toMatchObject({ text: 'not scored', value: null });
    expect(checksPassed([ev(2, 'check', { passed: true })])).toMatchObject({ text: '✓ passed', value: 1 });
    expect(safetyViolations([ev(2, 'check', { passed: true, checks: [{ name: 'x', passed: true }] })]).text).toBe('not scored');
    expect(kpiFigure(null, 'orders_covered').text).toBe('–');
    expect(lastView([])).toBeNull();
  });

  it('reads a twin\'s KPI by id, and nothing without it', () => {
    const v = lastView(v1Detail.events)!;
    expect(kpiFigure(v, 'transport_cost')).toMatchObject({ text: 'USD 3,410', value: 3410 });
    expect(kpiFigure({ ...v, kpis: [] }, 'transport_cost').text).toBe('–');
    expect(leadingNumber('USD 1,240.50')).toBe(1240.5);
    expect(leadingNumber('none')).toBeNull();
  });

  it('shows the cost the agents reported, labelled, and falls back to the metrics\' cost', () => {
    const side = (cost_usd: number | null, cost_source: string | null, metrics?: CompareSide['metrics']): CompareSide =>
      ({ status: 'passed', seconds: 1, tokens: 1200, cost_usd, cost_source, metrics });
    expect(agentCost(side(1.5, 'openrouter')).text).toBe('$1.50 (OpenRouter) · 1,200 tokens');
    expect(agentCost(side(1.5, 'my-gateway')).text).toBe('$1.50 (reported) · 1,200 tokens');
    expect(agentCost(side(null, null)).text).toBe('not reported · 1,200 tokens');
    const cost = { usd: 0.012, label: 'reported', source: 'my-gateway', as_of: null };
    const m = { measured_s: 1, overhead_ms: {}, overhead_s: 0, overhead_recorded: true, estimated_s: 1, tokens: 1200, cost, agents: [] };
    expect(agentCost(side(null, null, m))).toMatchObject({ text: '$0.012 (reported) · 1,200 tokens', value: 0.012 });
  });

  it('counts approval requests by agents, not the decisions links make', () => {
    expect(escalations(v1Detail.events).value).toBe(3);
  });

  it('reports a known agent cost', () => {
    const side: CompareSide = { ...comparison.a, cost_usd: 0.42, cost_source: 'openrouter', tokens: 1200 };
    const r = outcomeRows({ detail: v1Detail, side }, { detail: v2Detail, side: { ...side, cost_usd: 0.3 } }).find((x) => x.id === 'agent_cost')!;
    expect(r.a.text).toBe('$0.42 (OpenRouter) · 1,200 tokens');
    expect(verdict(r)).toBe('b');
  });
});

describe('outcome rows of a world with no twin', () => {
  // Two runs of a world whose checks measure figures (as ops-world's dc-shift does), with no twin
  // and no approvals system: only the rows they have.
  const checks = (total: number, passed: boolean) => [
    { name: 'Total cost below the FIFO baseline', passed, measure: { label: 'Total cost', value: total, unit: 'USD', better: 'lower' } },
    { name: 'Every late order\'s customer told', passed: true, measure: { label: 'Late orders', value: 4 } },
    { name: 'Stock never over-allocated', passed: true, safety: true },
  ];
  const side = (total: number, passed: boolean): SideInput => ({
    detail: { ...v1Detail, events: [ev(9, 'check', { passed, checks: checks(total, passed) })] },
    side: { ...comparison.a, cost_usd: 0.01, tokens: 100 },
  });
  const rows = outcomeRows(side(2191.5, true), side(5200, false));

  it('has a row for each figure a check measured, and none for figures neither run has', () => {
    expect(rows.map((r) => [r.label, r.a.text, r.b.text])).toEqual([
      ['Checks passed', '✓ 2/2', '✕ 1/2'],
      ['Safety violations', '0 of 1', '0 of 1'],
      ['Total cost', '2191.5 USD', '5200 USD'],
      ['Late orders', '4', '4'],
      ['Agent cost', '$0.01 (reported) · 100 tokens', '$0.01 (reported) · 100 tokens'],
    ]);
    expect(rows.map((r) => r.label)).not.toContain('Valid plan');
    expect(rows.map((r) => r.label)).not.toContain('Transport cost');
  });

  it('takes the better way from the measure, and none when the measure has none', () => {
    const r = rows.find((x) => x.id === 'measure:Total cost below the FIFO baseline')!;
    expect([verdict(r), r.b.abnormal]).toEqual(['a', true]);
    expect(rows.find((x) => x.label === 'Late orders')!.better).toBeNull();
    expect(measureFigure([], 'Total cost below the FIFO baseline').text).toBe('–');
  });
});

describe('divergence strip', () => {
  it('puts both branches on one step axis with the first divergence and their marks', () => {
    const s = divergenceStrip(v1Detail, v2Detail, comparison.first_divergent_seq);
    expect(s.first).toBe(FORK_SEQ);
    expect(s.min).toBe(3);
    expect(s.rows.map((r) => r.side)).toEqual(['A', 'B']);
    expect(s.rows[0].marks.map((m) => m.kind)).toEqual(['refusal', 'disruption']);
    expect(s.rows[1].marks.map((m) => m.kind)).toEqual(['fork', 'disruption']);
    expect(s.rows[0].end).toBe(s.max);
  });

  it('picks round ticks', () => {
    expect(axisTicks(1, 47)).toEqual([10, 20, 30, 40]);
    expect(axisTicks(0, 10, 5)).toEqual([0, 2, 4, 6, 8, 10]);
    expect(axisTicks(3, 3)).toEqual([3]);
  });
});
