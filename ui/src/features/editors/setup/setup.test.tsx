// @vitest-environment jsdom
// The agent setup editor: the team's kind, built-in agents, and credentials named, never written.
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { teamFor } from './teams';
import SetupEditor from './SetupEditor';

const BODY = { format: 'shakedown-setup/1', label: 'Solo planner', team: { kind: 'builtin', agents: [] } };
let puts: { body: Record<string, unknown> }[] = [];

function mockFetch(body: Record<string, unknown> = BODY) {
  puts = [];
  vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
    if (init?.method === 'PUT') { const sent = JSON.parse(String(init.body)); puts.push(sent); return new Response(JSON.stringify({ kind: 'setup', id: 'solo', label: 'S', hash: 'h', problems: [], body: sent.body })); }
    if (String(url).endsWith('/api/project/setups/solo')) return new Response(JSON.stringify({ kind: 'setup', id: 'solo', label: 'S', hash: 'h', problems: [], body }));
    return new Response(JSON.stringify({ error: 'not found' }), { status: 404 });
  }));
}

const show = () => render(<MemoryRouter initialEntries={['/setups/solo']}><Routes><Route path="/setups/:id" element={<SetupEditor />} /></Routes></MemoryRouter>);
const section = async (name: string) => fireEvent.click(within(await screen.findByRole('navigation', { name: 'Sections' })).getByRole('button', { name: new RegExp(`^${name}`) }));
const type = (label: string, value: string) => fireEvent.change(screen.getByLabelText(label), { target: { value } });
const save = async () => { fireEvent.click(await screen.findByRole('button', { name: 'Save' })); await waitFor(() => expect(puts.length).toBe(1)); return puts[0].body; };

afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe('setup editor', () => {
  it('switching_kind_resets_team', () => {
    expect(teamFor('http')).toEqual({ kind: 'http', url: '' });
    expect(teamFor('claude-code')).toEqual({ kind: 'claude-code' });
  });

  it('add_builtin_agent', async () => {
    mockFetch();
    show();
    await section('Agents');
    type('Agent id', 'planner');
    type('Role prompt', 'You keep orders on schedule.');
    type('Model', 'claude-sonnet');
    type('Model key name', 'anthropic-main');
    fireEvent.click(screen.getByRole('button', { name: 'Add agent' }));
    expect((await save()).team).toEqual({ kind: 'builtin', agents: [{ id: 'planner', role: 'You keep orders on schedule.', model: 'claude-sonnet', key: 'anthropic-main', tools: ['*'] }] });
  });

  it('http_auth_is_a_key_name', async () => {
    mockFetch();
    show();
    await section('How it runs');
    fireEvent.click(await screen.findByRole('radio', { name: /HTTP endpoint/ }));
    const key = await screen.findByLabelText('Key-store name');
    expect(key.getAttribute('placeholder')).toMatch(/key store/i);
    type('Endpoint URL', 'https://agents.example/planner');
    fireEvent.change(key, { target: { value: 'planner-endpoint' } });
    expect((await save()).team).toEqual({ kind: 'http', url: 'https://agents.example/planner', auth: { key: 'planner-endpoint' } });
  });
});

describe('MADL setups', () => {
  it('layout_columns', async () => {
    const { layout } = await import('./graph');
    const placed = layout({ agents: [{ id: 'a', label: 'A' }, { id: 'b', label: 'B' }, { id: 'c', label: 'C' }], edges: [{ from: 'a', to: 'b', kind: 'handoff' }, { from: 'b', to: 'c', kind: 'handoff' }] });
    expect(placed.nodes.map((n) => [n.id, n.column])).toEqual([['a', 0], ['b', 1], ['c', 2]]);
  });

  it('madl_section_lists_diagnostics', async () => {
    const report = { valid: false, diagnostics: [{ code: 'unknown-member', severity: 'error', pointer: '/topology/patterns/0/members/1', message: 'ghost is not an agent' }], graph: { agents: [], edges: [] } };
    vi.stubGlobal('fetch', vi.fn(async (url: string) => {
      const u = String(url);
      if (u.endsWith('/api/project/setups/solo/madl')) return new Response(JSON.stringify(report));
      if (u.endsWith('/api/project/setups/solo')) return new Response(JSON.stringify({ kind: 'setup', id: 'solo', label: 'S', hash: 'h', problems: [], body: { format: 'shakedown-setup/1', label: 'Team', team: { kind: 'madl', file: 'team.madl.yaml' } } }));
      return new Response(JSON.stringify({ error: 'not found' }), { status: 404 });
    }));
    show();
    await section('Team graph');
    expect(await screen.findByText('ghost is not an agent')).toBeTruthy();
    expect(screen.getByText(/unknown-member · \/topology/)).toBeTruthy();
  });
});
