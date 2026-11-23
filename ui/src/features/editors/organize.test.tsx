// @vitest-environment jsdom
// Deleting items (only if unused, or with everything that uses them) and the worlds list: groups,
// dragging a world onto a group, renaming a group, and worlds opening onto scenarios and experiments.
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { forgetShared } from '@/hooks/query';
import type { Dependent } from '@/model/project';
import DeleteItemDialog from './DeleteItemDialog';
import ItemListPage from './ItemListPage';
import { Toaster } from '@/components/ui/toaster';

const summary = (kind: string, id: string, label: string, more: Record<string, unknown> = {}) =>
  ({ kind, id, label, hash: 'h', problems: [], ...more });

const WORLDS = [
  summary('world', 'solo', 'Larkspur'),
  summary('world', 'plant-b', 'West plant', { group: 'Rebolt' }),
  summary('world', 'mill', 'Mill', { group: 'Arden' }),
  summary('world', 'plant-a', 'East plant', { group: 'Rebolt' }),
];
const SCENARIOS = [summary('scenario', 'jam', 'Belt jam', { world: 'mill' }), summary('scenario', 'leak', 'Leak', { world: 'solo' })];
const EXPERIMENTS = [summary('experiment', 'e1', 'Jam: solo vs team', { world: 'mill', scenario: 'jam' })];
const SOLO_BODY = { format: 'shakedown-world/3', label: 'Larkspur', connectors: [{ id: 'erp', label: 'ERP', mode: 'stand-in' }] };

interface Sent { url: string; method: string; body: unknown }
let sent: Sent[] = [];

const READS: Record<string, unknown> = {
  '/api/project/worlds': { items: WORLDS },
  '/api/project/scenarios': { items: SCENARIOS },
  '/api/project/experiments': { items: EXPERIMENTS },
  '/api/project/worlds/solo': { ...WORLDS[0], body: SOLO_BODY },
  '/api/project/worlds/mill': { ...WORLDS[2], body: { format: 'shakedown-world/3', label: 'Mill', group: 'Arden' } },
};

/** The answer to the next DELETE, when it is not a success. */
let refusal: { status: number; body: unknown } | null = null;

function mockFetch(dependents: Dependent[] = [], kept: string | null = null) {
  sent = [];
  refusal = null;
  vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
    const u = String(url);
    const method = init?.method ?? 'GET';
    if (method !== 'GET') {
      sent.push({ url: u, method, body: init?.body ? JSON.parse(String(init.body)) : null });
      const answer = method === 'DELETE' && refusal ? refusal : { status: 200, body: method === 'DELETE' ? { deleted: 'x', with: [], trash: '1' } : { moved: [] } };
      refusal = null;
      return new Response(JSON.stringify(answer.body), { status: answer.status });
    }
    if (u.endsWith('/dependents')) return new Response(JSON.stringify({ dependents, kept }));
    const key = Object.keys(READS).find((k) => u.endsWith(k));
    return key ? new Response(JSON.stringify(READS[key])) : new Response(JSON.stringify({ error: 'not found' }), { status: 404 });
  }));
}

beforeEach(() => forgetShared());
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

const openDialog = async (kind: 'worlds' | 'scenarios' | 'setups', id: string, label: string, onDeleted = vi.fn()) => {
  render(<MemoryRouter><DeleteItemDialog kind={kind} id={id} label={label} onDeleted={onDeleted} /></MemoryRouter>);
  fireEvent.click(screen.getByRole('button', { name: 'Delete' }));
  return { done: onDeleted, dialog: within(await screen.findByRole('dialog')) };
};

const WORLD_DEPS: Dependent[] = [
  { kind: 'scenario', id: 'jam', label: 'Belt jam', fate: 'deleted' },
  { kind: 'experiment', id: 'e1', label: 'Jam: solo vs team', fate: 'deleted' },
  { kind: 'experiment', id: 'e2', label: 'Jam: rules', fate: 'deleted' },
];

describe('delete dialog', () => {
  it('an_unused_item_has_one_delete_button', async () => {
    mockFetch([]);
    const { done, dialog } = await openDialog('scenarios', 'jam', 'Belt jam');
    expect(await dialog.findByText('Nothing uses this scenario.')).toBeTruthy();
    expect(dialog.queryByRole('button', { name: /^Delete (with|only)/ })).toBeNull();
    fireEvent.click(dialog.getByRole('button', { name: 'Delete' }));
    await waitFor(() => expect(done).toHaveBeenCalled());
    expect(sent).toHaveLength(1);
    expect(sent[0].method).toBe('DELETE');
    expect(sent[0].url.endsWith('/api/project/scenarios/jam')).toBe(true);
    expect(sent[0].body).toEqual({ cascade: false, confirm: true });
  });

  it('a_used_item_lists_dependents_and_deletes_them_with_what_it_was_shown', async () => {
    mockFetch(WORLD_DEPS, 'worlds/mill/');
    const { done, dialog } = await openDialog('worlds', 'mill', 'Mill');
    expect(await dialog.findByText('Belt jam')).toBeTruthy();
    expect(dialog.getByText('e2').className).toContain('font-mono');
    expect(dialog.getByText(/Earlier experiments/)).toBeTruthy();
    expect(dialog.getByText(/data folder worlds\/mill\/ stays/)).toBeTruthy();
    expect(dialog.getByText(/\.shakedown\/trash/)).toBeTruthy();
    expect((dialog.getByRole('button', { name: 'Delete only if unused' }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(dialog.getByRole('button', { name: 'Delete with 1 scenario, 2 experiments' }));
    await waitFor(() => expect(done).toHaveBeenCalled());
    expect(sent[0].body).toEqual({ cascade: true, confirm: true, expect: WORLD_DEPS });
  });

  it('a_stale_preview_shows_what_uses_it_now', async () => {
    mockFetch(WORLD_DEPS.slice(0, 2));
    const { done, dialog } = await openDialog('worlds', 'mill', 'Mill');
    refusal = { status: 409, body: { error: 'changed', dependents: WORLD_DEPS } };
    fireEvent.click(await dialog.findByRole('button', { name: 'Delete with 1 scenario, 1 experiment' }));
    expect(await dialog.findByText('Jam: rules')).toBeTruthy();
    expect(dialog.getByRole('alert').textContent).toMatch(/changed since this opened/);
    expect(done).not.toHaveBeenCalled();
    fireEvent.click(dialog.getByRole('button', { name: 'Delete with 1 scenario, 2 experiments' }));
    await waitFor(() => expect(done).toHaveBeenCalled());
    expect((sent[1].body as { expect: Dependent[] }).expect).toEqual(WORLD_DEPS);
  });

  it('setup_cascade_says_which_experiments_only_lose_it', async () => {
    mockFetch([
      { kind: 'experiment', id: 'e1', label: 'Compares three', fate: 'setup_dropped' },
      { kind: 'experiment', id: 'e2', label: 'Only this one', fate: 'deleted' },
    ], 'setups/solo/');
    const { dialog } = await openDialog('setups', 'solo', 'Solo planner');
    expect(await dialog.findByText('Compares three')).toBeTruthy();
    expect(dialog.getByText('keeps its other setups')).toBeTruthy();
    expect(dialog.getByText(/keep their other setups and only lose this one/)).toBeTruthy();
    expect(dialog.getByText(/setups\/solo\/ stays; only solo\.yaml is deleted/)).toBeTruthy();
    expect(dialog.getByRole('button', { name: 'Delete with 1 experiment and update 1 experiment' })).toBeTruthy();
  });

  it('every_fate_of_a_dependent_is_shown', async () => {
    mockFetch([{ kind: 'experiment', id: 'b', label: 'Branch', fate: 'setup_dropped', fates: ['setup_dropped', 'parent_cleared'] }]);
    const { dialog } = await openDialog('setups', 'x', 'X');
    expect(await dialog.findByText('keeps its other setups, becomes a top-level experiment')).toBeTruthy();
  });

  it('sub_experiments_are_said_to_stand_alone', async () => {
    mockFetch([{ kind: 'experiment', id: 'b', label: 'Branch', fate: 'parent_cleared' }]);
    const { dialog } = await openDialog('setups', 'x', 'X');
    expect(await dialog.findByText('becomes a top-level experiment')).toBeTruthy();
    expect(dialog.getByRole('button', { name: 'Delete and update 1 experiment' })).toBeTruthy();
  });
});

const showWorlds = async () => {
  render(<MemoryRouter><ItemListPage kind="worlds" /></MemoryRouter>);
  await screen.findByRole('link', { name: /^Larkspur/ });
};
const groupHeader = (name: string) => screen.getByRole('heading', { level: 3, name }).closest('[data-group]') as HTMLElement;
const rowOf = (label: string) => screen.getByRole('link', { name: new RegExp(`^${label}`) }).closest('li') as HTMLElement;

describe('worlds list', () => {
  it('worlds_under_sorted_group_headers', async () => {
    mockFetch();
    await showWorlds();
    expect(screen.getAllByRole('heading', { level: 3 }).map((h) => h.textContent)).toEqual(['Arden', 'Rebolt', 'Ungrouped']);
    const rebolt = within(groupHeader('Rebolt').parentElement as HTMLElement).getAllByRole('link').map((l) => l.textContent);
    expect(rebolt[0]).toMatch(/^East plant/);
    expect(rebolt[1]).toMatch(/^West plant/);
  });

  it('drag_world_onto_group_moves_it', async () => {
    mockFetch();
    await showWorlds();
    fireEvent.dragStart(rowOf('Larkspur'));
    fireEvent.dragOver(groupHeader('Arden'));
    fireEvent.drop(groupHeader('Arden'));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].method).toBe('PUT');
    expect(sent[0].url.endsWith('/api/project/worlds/solo')).toBe(true);
    expect(sent[0].body).toEqual({ body: { ...SOLO_BODY, group: 'Arden' }, confirm: true });
  });

  it('drag_world_onto_ungrouped_clears_its_group', async () => {
    mockFetch();
    await showWorlds();
    fireEvent.dragStart(rowOf('Mill'));
    fireEvent.drop(groupHeader('Ungrouped'));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].body).toEqual({ body: { format: 'shakedown-world/3', label: 'Mill' }, confirm: true });
  });

  it('drag_shows_undo_which_moves_it_back', async () => {
    mockFetch();
    render(<MemoryRouter><ItemListPage kind="worlds" /><Toaster /></MemoryRouter>);
    await screen.findByRole('link', { name: /^Larkspur/ });
    fireEvent.dragStart(rowOf('Mill'));
    fireEvent.drop(groupHeader('Rebolt'));
    const moved = (await screen.findByText('Moved Mill to Rebolt')).closest('[data-sonner-toast]') as HTMLElement;
    fireEvent.click(within(moved).getByRole('button', { name: 'Undo' }));
    await waitFor(() => expect(sent).toHaveLength(2));
    expect(sent[1].body).toEqual({ body: { format: 'shakedown-world/3', label: 'Mill', group: 'Arden' }, confirm: true });
  });

  it('rename_into_an_existing_group_says_so', async () => {
    mockFetch();
    await showWorlds();
    fireEvent.click(screen.getByRole('button', { name: 'Rename group Rebolt' }));
    fireEvent.change(screen.getByLabelText('Group name'), { target: { value: 'Arden' } });
    expect(screen.getByText('Moves 2 worlds into Arden, which already exists.')).toBeTruthy();
  });

  it('rename_group_from_its_header', async () => {
    mockFetch();
    await showWorlds();
    fireEvent.click(screen.getByRole('button', { name: 'Rename group Rebolt' }));
    fireEvent.change(screen.getByLabelText('Group name'), { target: { value: 'Rebolt Foods' } });
    fireEvent.click(screen.getByRole('button', { name: 'Rename' }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].url.endsWith('/api/project/world-groups/rename')).toBe(true);
    expect(sent[0].body).toEqual({ from: 'Rebolt', to: 'Rebolt Foods', confirm: true });
  });

  it('world_opens_onto_scenarios_then_experiments', async () => {
    mockFetch();
    await showWorlds();
    expect(screen.queryByRole('link', { name: /^Belt jam/ })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Show scenarios in Mill' }));
    expect((await screen.findByRole('link', { name: /^Belt jam/ })).getAttribute('href')).toBe('/scenarios/jam');
    expect(screen.queryByRole('link', { name: /^Leak/ })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Show experiments using Belt jam' }));
    expect((await screen.findByRole('link', { name: /^Jam: solo vs team/ })).getAttribute('href')).toBe('/experiments/file/e1');
  });

  it('each_row_has_a_delete_action', async () => {
    mockFetch();
    await showWorlds();
    expect(screen.getByRole('button', { name: 'Delete Larkspur' })).toBeTruthy();
  });
});
