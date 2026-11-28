// @vitest-environment jsdom
// Project settings: permissions and the assistant's model saved to the project file, a spend cap
// that must be a positive number, and an MCP command with no folder in it.
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { forgetShared } from '@/hooks/query';
import type { ProjectSettings } from '@/model/project';
import { settingsFrom } from './project';
import ProjectSettingsPage from './ProjectSettingsPage';

const SETTINGS: ProjectSettings = { format: 'shakedown-project/1', name: 'Larkspur Ops', permissions: { edit: 'diff_first', spend: 'always_ask' } };
let puts: { body: Record<string, unknown> }[] = [];
let posts: { url: string; body: unknown }[] = [];

function mockFetch() {
  puts = [];
  posts = [];
  forgetShared();
  vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
    if (init?.method === 'POST') { posts.push({ url: String(url), body: JSON.parse(String(init.body)) }); return new Response(JSON.stringify({ restored: 'world mill', files: [] })); }
    if (String(url).endsWith('/api/project/trash')) return new Response(JSON.stringify({ trash: '1763900000000', label: 'world mill', files: ['worlds/mill/world.yaml'] }));
    if (init?.method === 'PUT') { const sent = JSON.parse(String(init.body)); puts.push(sent); return new Response(JSON.stringify({ name: 'L', counts: {}, problems: [], settings: sent.body })); }
    if (String(url).endsWith('/api/project')) return new Response(JSON.stringify({ name: 'Larkspur Ops', counts: {}, problems: [], settings: SETTINGS }));
    return new Response(JSON.stringify({ error: 'not found' }), { status: 404 });
  }));
}

const section = async (name: string) => fireEvent.click(within(await screen.findByRole('navigation', { name: 'Sections' })).getByRole('button', { name: new RegExp(`^${name}`) }));

afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe('project settings', () => {
  it('under_cap_needs_a_number', () => {
    expect(settingsFrom(SETTINGS, { edit: 'diff_first', spendMode: 'under_cap', cap: '10', name: 'L', model: '', provider: '' }))
      .toMatchObject({ permissions: { spend: { under_cap: { usd_per_day: 10 } } } });
  });

  it('bad_cap_refused', () => {
    expect(() => settingsFrom(SETTINGS, { edit: 'diff_first', spendMode: 'under_cap', cap: '-3', name: 'L', model: '', provider: '' })).toThrow(/positive number/);
    expect(() => settingsFrom(SETTINGS, { edit: 'diff_first', spendMode: 'under_cap', cap: 'ten', name: 'L', model: '', provider: '' })).toThrow(/positive number/);
  });

  it('loads_and_saves_permissions', async () => {
    mockFetch();
    render(<MemoryRouter><ProjectSettingsPage /></MemoryRouter>);
    await section('Permissions');
    fireEvent.click(await screen.findByRole('radio', { name: 'Apply, then tell me' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Save' }));
    await waitFor(() => expect(puts.length).toBe(1));
    expect(puts[0].body.permissions).toEqual({ edit: 'apply_then_tell', spend: 'always_ask' });
  });

  it('mcp_command_has_no_path', async () => {
    mockFetch();
    render(<MemoryRouter><ProjectSettingsPage /></MemoryRouter>);
    await section('Use from an agent');
    const code = await screen.findByText(/claude mcp add shakedown/);
    expect(code.textContent).toBe('claude mcp add shakedown -- shaked mcp');
  });

  it('undo_last_delete_restores_that_delete', async () => {
    mockFetch();
    render(<MemoryRouter><ProjectSettingsPage /></MemoryRouter>);
    await section('Deleted files');
    expect(await screen.findByText('world mill')).toBeTruthy();
    expect(screen.getByText('worlds/mill/world.yaml')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Undo last delete' }));
    await waitFor(() => expect(posts).toHaveLength(1));
    expect(posts[0].url.endsWith('/api/project/trash/restore')).toBe(true);
    expect(posts[0].body).toEqual({ trash: '1763900000000', confirm: true });
  });
});
