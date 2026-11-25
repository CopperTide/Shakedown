// @vitest-environment jsdom
// Filtering a run's steps: by action, agent and outcome with counts, by text in the detail, kept in
// the address; the event log lists the matches and the timeline dims the rest.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter, useLocation } from 'react-router-dom';
import { afterEach, describe, expect, it } from 'vitest';
import { events, team } from '@/model/fixtures';
import { buildLanes, spanOf } from '@/model/lanes';
import ActionTimeline from './ActionTimeline';
import EventLog from './EventLog';
import { actionOf, chipsOf, isActive, logRows, matching, NO_FILTER, parseFilter, toggled, writeFilter } from './logFilter';
import { LogFilters, useLogFilter } from './LogFilters';

const lanes = buildLanes(events, team);
const rows = logRows(lanes, true);
const seqs = (s: Set<number> | null) => (s ? [...s].sort((a, b) => a - b) : null);

class NoResize { observe() {} disconnect() {} unobserve() {} }
globalThis.ResizeObserver ??= NoResize as unknown as typeof ResizeObserver;

afterEach(cleanup);

describe('step filter', () => {
  it('matches_any_picked_value_of_each_facet_and_text_in_the_detail', () => {
    expect(matching(rows, NO_FILTER)).toBeNull();
    expect(seqs(matching(rows, { ...NO_FILTER, outcome: ['refused'] }))).toEqual([9]);
    const quality = { ...NO_FILTER, agent: ['Quality'], q: 'HOLD' };
    expect(seqs(matching(rows, quality))).toEqual([9]);
    const calls = matching(rows, { ...NO_FILTER, action: ['Model call'] });
    expect(seqs(calls)).toEqual([3, 10]);
    expect(actionOf({ ...rows[0].m, label: 'Message → quality' })).toBe('Message');
  });

  it('counts_each_value_under_the_rest_of_the_filter', () => {
    const f = { ...NO_FILTER, agent: ['Coordinator'] };
    const chips = chipsOf(rows, f);
    expect(chips.agent.find((c) => c.value === 'Quality')?.count).toBeGreaterThan(0);
    expect(chips.agent.find((c) => c.value === 'Coordinator')?.on).toBe(true);
    expect(chips.action.find((c) => c.value === 'Model call')?.count).toBe(1);
    expect(chipsOf(rows, { ...NO_FILTER, outcome: ['gone'] }).outcome.find((c) => c.value === 'gone')).toEqual({ value: 'gone', count: 0, on: true });
  });

  it('round_trips_through_the_address_keeping_other_parameters', () => {
    const f = toggled(toggled({ ...NO_FILTER, q: 'erp' }, 'action', 'Reasoning'), 'action', 'Model call');
    const sp = writeFilter(new URLSearchParams('step=4'), f);
    expect(sp.get('step')).toBe('4');
    expect(parseFilter(sp)).toEqual({ ...NO_FILTER, action: ['Reasoning', 'Model call'], q: 'erp' });
    const cleared = writeFilter(sp, NO_FILTER);
    expect([...cleared.keys()]).toEqual(['step']);
    expect(isActive(parseFilter(cleared))).toBe(false);
  });
});

function Where() {
  return <output data-testid="search">{useLocation().search}</output>;
}

function Harness() {
  const { filter, setFilter, matches } = useLogFilter(rows);
  return (
    <>
      <LogFilters rows={rows} filter={filter} setFilter={setFilter} shown={matches?.size ?? null} />
      <ActionTimeline lanes={lanes} team={team} span={spanOf(lanes)} selected={null} onSelect={() => {}} models onModels={() => {}} matches={matches} />
      <EventLog rows={rows} matches={matches} span={spanOf(lanes)} selected={null} onSelect={() => {}} />
      <Where />
    </>
  );
}

describe('step filter on the run page', () => {
  it('opens_from_a_shared_address_lists_the_matches_and_dims_the_rest_of_the_timeline', () => {
    render(<MemoryRouter initialEntries={['/runs/r/b?outcome=refused']}><Harness /></MemoryRouter>);
    expect(screen.getByRole('status', { name: 'Matching steps' }).textContent).toBe(`1 of ${rows.length} steps`);
    expect(screen.getAllByRole('row')).toHaveLength(2); // the header and step 9
    expect(document.querySelector('[data-seq="9"]')?.hasAttribute('data-dim')).toBe(false);
    expect(document.querySelector('[data-seq="6"]')?.hasAttribute('data-dim')).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: 'Agent' }));
    const lead = screen.getByRole('checkbox', { name: /Coordinator/ });
    expect(lead.closest('label')?.textContent).toBe('Coordinator0');
  });

  it('picks_chips_searches_and_clears_all_in_the_address', () => {
    render(<MemoryRouter initialEntries={['/runs/r/b?step=3']}><Harness /></MemoryRouter>);
    fireEvent.click(screen.getByRole('button', { name: 'Action' }));
    fireEvent.change(screen.getByLabelText('Find action options'), { target: { value: 'Model' } });
    fireEvent.click(screen.getByRole('checkbox', { name: /^Model call/ }));
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(screen.getByTestId('search').textContent).toBe('?step=3&action=Model+call');
    expect(screen.getAllByRole('row')).toHaveLength(3);
    fireEvent.change(screen.getByLabelText("Search the steps' detail"), { target: { value: 'nothing like it' } });
    expect(screen.getByText('No step matches the filter.')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Clear all' }));
    expect(screen.getByTestId('search').textContent).toBe('?step=3');
    expect(screen.getAllByRole('row')).toHaveLength(rows.length + 1);
  });
});
