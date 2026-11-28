// @vitest-environment jsdom
// From a world to its scenarios and from a scenario to its experiments, a world's group chosen in
// its editor, and a delete action on each editor page.
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { forgetShared } from '@/hooks/query';
import ScenarioEditor from './scenario/ScenarioEditor';
import WorldEditor from './world/WorldEditor';

const summary = (kind: string, id: string, label: string, more: Record<string, unknown> = {}) =>
  ({ kind, id, label, hash: 'h', problems: [], ...more });

const WORLD = { format: 'shakedown-world/3', label: 'Mill' };
const SCENARIO = { format: 'shakedown-scenario/1', label: 'Belt jam', world: 'mill', task: 'Clear the jam.' };
let puts: { body: Record<string, unknown> }[] = [];

function mockFetch() {
  puts = [];
  vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
    const u = String(url);
    if (init?.method === 'PUT') {
      const body = JSON.parse(String(init.body));
      puts.push(body);
      return new Response(JSON.stringify({ ...summary('world', 'mill', 'Mill'), body: body.body }));
    }
    const answers: Record<string, unknown> = {
      '/api/project/worlds/mill': { ...summary('world', 'mill', 'Mill'), body: WORLD },
      '/api/project/scenarios/jam': { ...summary('scenario', 'jam', 'Belt jam'), body: SCENARIO },
      '/api/project/worlds': { items: [summary('world', 'mill', 'Mill'), summary('world', 'b', 'B', { group: 'Rebolt' }), summary('world', 'a', 'A', { group: 'Arden' })] },
      '/api/project/scenarios': { items: [summary('scenario', 'leak', 'Tank leak', { world: 'mill' }), summary('scenario', 'jam', 'Belt jam', { world: 'mill' }), summary('scenario', 'x', 'Elsewhere', { world: 'b' })] },
      '/api/project/experiments': { items: [summary('experiment', 'e1', 'Jam: solo vs team', { world: 'mill', scenario: 'jam' }), summary('experiment', 'e2', 'Leak test', { world: 'mill', scenario: 'leak' })] },
    };
    const key = Object.keys(answers).find((k) => u.endsWith(k));
    return key ? new Response(JSON.stringify(answers[key])) : new Response(JSON.stringify({ error: 'not found' }), { status: 404 });
  }));
}

const show = (path: string) => render(
  <MemoryRouter initialEntries={[path]}>
    <Routes><Route path="/worlds/:id" element={<WorldEditor />} /><Route path="/scenarios/:id" element={<ScenarioEditor />} /></Routes>
  </MemoryRouter>,
);

beforeEach(() => forgetShared());
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe('hierarchy', () => {
  it('world_page_lists_its_scenarios', async () => {
    mockFetch();
    show('/worlds/mill');
    expect(await screen.findByText('Scenarios')).toBeTruthy();
    expect(await screen.findByText('2 scenarios')).toBeTruthy();
    const links = (await screen.findAllByRole('link', { name: /Belt jam|Tank leak|Elsewhere/ }));
    expect(links.map((l) => l.textContent)).toEqual(['Belt jam', 'Tank leak']);
    expect(links[0].getAttribute('href')).toBe('/scenarios/jam');
  });

  it('scenario_page_lists_its_experiments', async () => {
    mockFetch();
    show('/scenarios/jam');
    expect(await screen.findByText('Experiments', { selector: 'h3' })).toBeTruthy();
    const link = await screen.findByRole('link', { name: /^Jam: solo vs team/ });
    expect(link.getAttribute('href')).toBe('/experiments/file/e1');
    expect(screen.queryByRole('link', { name: /^Leak test/ })).toBeNull();
  });

  it('editor_pages_offer_delete', async () => {
    mockFetch();
    show('/scenarios/jam');
    expect(await screen.findByRole('button', { name: 'Delete' })).toBeTruthy();
  });
});

describe('world group field', () => {
  it('choose_an_existing_group', async () => {
    mockFetch();
    show('/worlds/mill');
    const field = await screen.findByLabelText('Group') as HTMLSelectElement;
    await waitFor(() => expect(Array.from(field.options).map((o) => o.textContent)).toEqual(['No group', 'Arden', 'Rebolt', 'New group…']));
    fireEvent.change(field, { target: { value: 'Rebolt' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(puts).toHaveLength(1));
    expect(puts[0].body.group).toBe('Rebolt');
  });

  it('make_a_new_group', async () => {
    mockFetch();
    show('/worlds/mill');
    const field = await screen.findByLabelText('Group') as HTMLSelectElement;
    await waitFor(() => expect(field.options.length).toBe(4));
    const fresh = Array.from(field.options).find((o) => o.textContent === 'New group…');
    fireEvent.change(field, { target: { value: fresh?.value } });
    fireEvent.change(screen.getByLabelText('New group name'), { target: { value: ' Faraday ' } });
    fireEvent.blur(screen.getByLabelText('New group name'));
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(puts).toHaveLength(1));
    expect(puts[0].body.group).toBe('Faraday');
  });
});
