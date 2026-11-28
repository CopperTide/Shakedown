// @vitest-environment jsdom
// The scenario's task: picked from the tasks of the world's package, searched and grouped, the
// current value shown even when the package lacks it, and typed in when the tasks can't be read.
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { ScenarioBody } from '@/model/project';
import InputsSection from './InputsSection';
import { taskGroups } from './tasks';

const TASKS = [
  { id: '0', reason: 'Cancel a pending order', group: 'test' },
  { id: '1', reason: 'Exchange two items', group: 'test' },
  { id: '7', reason: 'Refund a delivered order', group: 'train' },
];
const BODY: ScenarioBody = { format: 'shakedown-scenario/1', label: 'S', world: 'shop', task: 'Help the customer.', preset: '1' };

function mockFetch(bench: { status: number; body: unknown }) {
  vi.stubGlobal('fetch', vi.fn(async (url: string) => {
    const u = String(url);
    if (u.endsWith('/api/project/worlds')) return new Response(JSON.stringify({ items: [{ id: 'shop', label: 'Shop', problems: [] }] }));
    if (u.includes('/api/benchmark?world=shop')) return new Response(JSON.stringify(bench.body), { status: bench.status });
    return new Response(JSON.stringify({ error: 'not found' }), { status: 404 });
  }));
}

const LISTED = { status: 200, body: { name: 'retail', version: '1', tasks: TASKS, scenario_options: null, view: false } };
const LABEL = 'Task';

function show(scenario: ScenarioBody = BODY) {
  const onChange = vi.fn();
  render(<InputsSection scenario={scenario} onChange={onChange} />);
  return onChange;
}

afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe('task picker', () => {
  it('task_groups_filter_by_id_or_reason_in_first_seen_order', () => {
    expect(taskGroups(TASKS, '').map((g) => [g.group, g.tasks.length])).toEqual([['test', 2], ['train', 1]]);
    expect(taskGroups(TASKS, 'ORDER').map((g) => [g.group, g.tasks.map((t) => t.id)])).toEqual([['test', ['0']], ['train', ['7']]]);
    expect(taskGroups(TASKS, '7')).toEqual([{ group: 'train', tasks: [TASKS[2]] }]);
    expect(taskGroups([{ id: 'a', reason: 'x' }], '')).toEqual([{ group: null, tasks: [{ id: 'a', reason: 'x' }] }]);
  });

  it('shows_the_current_task_and_picks_another_from_a_searched_group', async () => {
    mockFetch(LISTED);
    const onChange = show();
    const button = await screen.findByRole('button', { name: new RegExp(LABEL) });
    expect(button.textContent).toContain('Exchange two items');
    fireEvent.click(button);
    const list = screen.getByRole('listbox');
    expect(within(list).getByRole('group', { name: 'test' })).toBeTruthy();
    fireEvent.change(screen.getByRole('searchbox', { name: 'Search tasks' }), { target: { value: 'refund' } });
    expect(within(list).queryByRole('group', { name: 'test' })).toBeNull();
    fireEvent.click(within(within(list).getByRole('group', { name: 'train' })).getByRole('option', { name: /Refund a delivered order/ }));
    expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ preset: '7' }));
    expect(screen.queryByRole('listbox')).toBeNull();
  });

  it('a_current_task_the_package_lacks_is_still_shown', async () => {
    mockFetch(LISTED);
    show({ ...BODY, preset: 'gone-9' });
    const button = await screen.findByRole('button', { name: new RegExp(LABEL) });
    expect(button.textContent).toContain('gone-9');
    expect(screen.getByText('Not a task of retail.')).toBeTruthy();
  });

  it('falls_back_to_free_text_when_the_tasks_cant_be_read', async () => {
    mockFetch({ status: 404, body: { error: 'no benchmark named retail is installed' } });
    const onChange = show();
    expect(await screen.findByText(/no benchmark named retail is installed/)).toBeTruthy();
    fireEvent.change(screen.getByLabelText(LABEL), { target: { value: 't9' } });
    expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ preset: 't9' }));
  });
});
