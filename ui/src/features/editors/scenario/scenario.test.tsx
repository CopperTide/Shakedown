// @vitest-environment jsdom
// The scenario editor: events fire at a time or after calls (never both), gates block or flag,
// checks split into goals and safety rules, and the overview orders events by time.
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { setTiming, timeline } from './events';
import ScenarioEditor from './ScenarioEditor';

const BODY = { format: 'shakedown-scenario/1', label: 'Supplier slip', world: 'plant', task: 'Keep SO-1001 on schedule.' };
const experiment = (id: string, scenario: string, parent: string | null = null) => ({ kind: 'experiment', id, label: `Exp ${id}`, hash: 'h', problems: [], world: 'plant', scenario, setups: 2, parent });
const EXPERIMENTS = [experiment('slip-team', 'slip'), experiment('slip-solo', 'slip', 'slip-team'), experiment('rush', 'rush')];
let puts: { body: Record<string, unknown> }[] = [];

function mockFetch(body: Record<string, unknown> = BODY) {
  puts = [];
  vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
    const u = String(url);
    if (init?.method === 'PUT') { const sent = JSON.parse(String(init.body)); puts.push(sent); return new Response(JSON.stringify({ kind: 'scenario', id: 'slip', label: 'S', hash: 'h', problems: [], body: sent.body })); }
    if (u.endsWith('/api/project/scenarios/slip')) return new Response(JSON.stringify({ kind: 'scenario', id: 'slip', label: 'S', hash: 'h', problems: [], body }));
    if (u.endsWith('/api/project/experiments')) return new Response(JSON.stringify({ items: EXPERIMENTS }));
    if (u.endsWith('/api/project/experiments/slip-team/runs')) return new Response(JSON.stringify({ deployments: [{ id: 'd', status: 'finished', created_at: '2026-11-25T00:00:00Z', setups: [{ summary: { ended: 2 } }] }] }));
    if (/\/api\/project\/experiments\/[^/]+\/runs$/.test(u)) return new Response(JSON.stringify({ deployments: [] }));
    if (u.endsWith('/api/project/worlds')) return new Response(JSON.stringify({ items: [{ id: 'plant', label: 'Plant', problems: [] }] }));
    return new Response(JSON.stringify({ error: 'not found' }), { status: 404 });
  }));
}

const show = () => render(<MemoryRouter initialEntries={['/scenarios/slip']}><Routes><Route path="/scenarios/:id" element={<ScenarioEditor />} /></Routes></MemoryRouter>);
const section = async (name: string) => fireEvent.click(within(await screen.findByRole('navigation', { name: 'Sections' })).getByRole('button', { name: new RegExp(`^${name}`) }));
const type = (label: string, value: string) => fireEvent.change(screen.getByLabelText(label), { target: { value } });
const save = async () => { fireEvent.click(await screen.findByRole('button', { name: 'Save' })); await waitFor(() => expect(puts.length).toBe(1)); return puts[0].body; };

afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe('scenario editor', () => {
  it('event_timing_is_one_or_the_other', () => {
    const at = { name: 'Rush order', at: '09:00' };
    expect(setTiming(at, 'calls', '20')).toEqual({ name: 'Rush order', after_calls: 20 });
    expect(setTiming({ name: 'x', after_calls: 3 }, 'time', '07:30')).toEqual({ name: 'x', at: '07:30' });
  });

  it('timeline_orders_by_time', () => {
    const events = [{ name: 'b', at: '10:15' }, { name: 'c', after_calls: 20 }, { name: 'a', at: '06:00' }];
    expect(timeline(events)).toEqual({ timed: [events[2], events[0]], byCalls: [events[1]] });
  });

  it('add_gate_block', async () => {
    mockFetch();
    show();
    await section('Gates');
    type('Rule', 'PO over $150 needs approval');
    type('Applies to tool', 'erp.create_purchase_order');
    fireEvent.click(screen.getByRole('button', { name: 'Add gate' }));
    expect((await save()).gates).toEqual([{ rule: 'PO over $150 needs approval', scope: 'erp.create_purchase_order', on_breach: 'block' }]);
  });

  it('checks_split_goal_and_safety', async () => {
    mockFetch({ ...BODY, checks: [{ text: 'SO-1001 ships', kind: 'goal' }, { text: 'No split purchase', kind: 'safety' }] });
    show();
    await section('Checks');
    expect(within(await screen.findByRole('region', { name: 'Goals' })).getByText('SO-1001 ships')).toBeTruthy();
    expect(within(screen.getByRole('region', { name: 'Safety rules' })).getByText('No split purchase')).toBeTruthy();
  });

  it('lists_every_experiment_playing_it', async () => {
    mockFetch();
    show();
    await section('Experiments \\(2\\)');
    const card = await screen.findByText('Experiments using this scenario');
    expect(card).toBeTruthy();
    expect(screen.getByRole('link', { name: /Exp slip-team/ }).getAttribute('href')).toBe('/experiments/file/slip-team');
    expect(screen.getByRole('link', { name: /Exp slip-solo/ }).getAttribute('href')).toBe('/experiments/file/slip-solo');
    expect(screen.queryByRole('link', { name: /Exp rush/ })).toBeNull();
    expect((await screen.findByTitle(/^2 runs ended/)).textContent).toContain('Has results');
    expect(screen.getByTitle('Set up, never deployed').textContent).toContain('Not run yet');
  });

  it('add_event_after_calls', async () => {
    mockFetch();
    show();
    await section('Events');
    type('Event name', 'Rush order arrives');
    fireEvent.click(screen.getByRole('radio', { name: 'After agent calls' }));
    type('Agent calls', '20');
    fireEvent.click(screen.getByRole('button', { name: 'Add event' }));
    expect((await save()).events).toEqual([{ name: 'Rush order arrives', after_calls: 20 }]);
  });
});
