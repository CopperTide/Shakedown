// @vitest-environment jsdom
// The editing layer: a draft that saves through the project API, keeps itself through a refused
// save, discards back to the file, and new items that check their id first.
import { act, cleanup, fireEvent, render, renderHook, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { forgetShared } from '@/hooks/query';
import ItemListPage from './ItemListPage';
import { KINDS } from './kinds';
import { useItemEditor } from './useItemEditor';

const WORLD = { kind: 'world', id: 'plant', label: 'Plant', hash: 'h', problems: [] as { pointer: string; message: string }[], body: { format: 'shakedown-world/3', label: 'Plant' } };
const puts: unknown[] = [];

function mockFetch({ put = { status: 200, body: WORLD as unknown }, items = [WORLD] } = {}) {
  puts.length = 0;
  vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
    const u = String(url);
    if (init?.method === 'PUT') { puts.push(JSON.parse(String(init.body))); return new Response(JSON.stringify(put.body), { status: put.status }); }
    if (u.endsWith('/api/project/worlds')) return new Response(JSON.stringify({ items }));
    if (u.endsWith('/api/project/worlds/plant')) return new Response(JSON.stringify(WORLD));
    if (u.endsWith('/api/worlds')) return new Response(JSON.stringify([{ name: 'tau-airline', title: 'tau airline', built: false }]));
    return new Response(JSON.stringify({ error: 'not found' }), { status: 404 });
  }));
}

beforeEach(() => forgetShared());
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

async function editor() {
  const hook = renderHook(() => useItemEditor<{ label: string }>('worlds', 'plant'));
  await waitFor(() => expect(hook.result.current.draft).not.toBeNull());
  return hook;
}

describe('editing layer', () => {
  it('save_error_keeps_draft', async () => {
    mockFetch({ put: { status: 422, body: { error: 'w.yaml at /connectors/0/auth/key: this looks like a credential' } } });
    const { result } = await editor();
    act(() => result.current.setDraft({ label: 'Renamed' }));
    await act(() => result.current.save());
    expect(result.current.draft?.label).toBe('Renamed');
    expect(result.current.dirty).toBe(true);
    expect(result.current.error?.message).toContain('this looks like a credential');
    expect(result.current.error?.pointer).toBe('/connectors/0/auth/key');
  });

  it('discard_restores', async () => {
    mockFetch();
    const { result } = await editor();
    act(() => result.current.setDraft({ label: 'Changed' }));
    expect(result.current.dirty).toBe(true);
    act(() => result.current.discard());
    expect(result.current.draft?.label).toBe('Plant');
    expect(result.current.dirty).toBe(false);
  });

  it('save_sends_confirm', async () => {
    mockFetch();
    const { result } = await editor();
    act(() => result.current.setDraft({ label: 'Saved' }));
    await act(() => result.current.save());
    expect(puts[0]).toEqual({ body: { label: 'Saved' }, confirm: true });
  });

  it('list_row_is_one_link_to_editor_and_highlights_on_hover', async () => {
    mockFetch({ items: [{ ...WORLD, problems: [{ pointer: '', message: 'broken' }] }] });
    render(<MemoryRouter><ItemListPage kind="worlds" /></MemoryRouter>);
    const link = await screen.findByRole('link', { name: /^Plant/ });
    expect(link.getAttribute('href')).toBe('/worlds/plant');
    expect(link.textContent).toContain('1 problem');
    expect(link.closest('li')?.className).toContain('hover:bg-hover');
  });

  it('new_item_bad_id', async () => {
    mockFetch();
    render(<MemoryRouter><ItemListPage kind="worlds" /></MemoryRouter>);
    fireEvent.click(await screen.findByRole('button', { name: /New world/ }));
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'Second plant' } });
    fireEvent.change(screen.getByLabelText('Id'), { target: { value: 'My World' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create' }));
    expect(await screen.findByText(/lowercase letters, digits and dashes/)).toBeTruthy();
    expect(puts).toEqual([]);
  });

  it('new_world_from_scratch_on_an_installed_runtime', async () => {
    mockFetch({ put: { status: 200, body: { ...WORLD, id: 'airline' } } });
    render(<MemoryRouter><ItemListPage kind="worlds" /></MemoryRouter>);
    fireEvent.click(await screen.findByRole('button', { name: /New world/ }));
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'Airline' } });
    await screen.findByRole('option', { name: 'tau airline (tau-airline)' });
    fireEvent.change(screen.getByLabelText('Runtime'), { target: { value: 'tau-airline' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create' }));
    await waitFor(() => expect(puts.length).toBe(1));
    expect(puts[0]).toEqual({ body: { format: 'shakedown-world/3', label: 'Airline', package: 'tau-airline' }, confirm: true });
  });

  it('new_item_id_taken', async () => {
    mockFetch();
    render(<MemoryRouter><ItemListPage kind="worlds" /></MemoryRouter>);
    fireEvent.click(await screen.findByRole('button', { name: /New world/ }));
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'Plant again' } });
    fireEvent.change(screen.getByLabelText('Id'), { target: { value: 'plant' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create' }));
    expect(await screen.findByText('A world named plant already exists.')).toBeTruthy();
    expect(puts).toEqual([]);
  });

  it('each_kind_suggests_a_name_of_its_own', async () => {
    mockFetch();
    render(<MemoryRouter><ItemListPage kind="scenarios" /></MemoryRouter>);
    fireEvent.click(await screen.findByRole('button', { name: /New scenario/ }));
    expect(screen.getByLabelText('Name').getAttribute('placeholder')).toBe('Supplier slip during a rush order');
    const examples = Object.values(KINDS).map((k) => k.example);
    expect(new Set(examples).size).toBe(examples.length);
  });
});
