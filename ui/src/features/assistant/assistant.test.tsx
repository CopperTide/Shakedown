// @vitest-environment jsdom
// The Ask drawer: opened from the top bar, sends the page with the question, shows the reply and
// the tools used, and applies a card only when the person clicks.
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ASK_EVENT } from '@/features/shell/TopBar';
import { PROJECT_CHANGED } from '@/hooks/useProjectItems';
import AskDrawer from './AskDrawer';

const calls: { url: string; method: string; body: unknown }[] = [];
const DEPS = [{ kind: 'scenario', id: 'jam', label: 'Belt jam', fate: 'deleted' }];

function mockFetch(answer: Record<string, unknown>) {
  calls.length = 0;
  vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
    const u = String(url);
    calls.push({ url: u, method: init?.method ?? 'GET', body: init?.body ? JSON.parse(String(init.body)) : null });
    if (u.endsWith('/api/assistant/chat')) return new Response(JSON.stringify({ messages: [], tools: [], cards: [], ...answer }));
    if (u.endsWith('/dependents')) return new Response(JSON.stringify({ dependents: DEPS, kept: 'worlds/mill/' }));
    return new Response(JSON.stringify({ ok: true }));
  }));
}

async function openAndAsk(text: string) {
  render(<MemoryRouter initialEntries={['/experiments']}><AskDrawer /></MemoryRouter>);
  act(() => { window.dispatchEvent(new CustomEvent(ASK_EVENT)); });
  fireEvent.change(await screen.findByLabelText('Ask the assistant'), { target: { value: text } });
  fireEvent.click(screen.getByRole('button', { name: 'Send' }));
}

afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe('ask drawer', () => {
  it('ask_event_opens_drawer', async () => {
    mockFetch({ reply: '' });
    render(<MemoryRouter><AskDrawer /></MemoryRouter>);
    expect(screen.queryByRole('complementary', { name: 'Assistant' })).toBeNull();
    act(() => { window.dispatchEvent(new CustomEvent(ASK_EVENT)); });
    expect(await screen.findByRole('complementary', { name: 'Assistant' })).toBeTruthy();
  });

  it('sends_page_and_shows_reply', async () => {
    mockFetch({ reply: 'One world.', tools: [{ name: 'worlds_list', summary: 'Lists them' }] });
    await openAndAsk('How many worlds?');
    expect(await screen.findByText('One world.')).toBeTruthy();
    expect(screen.getByText('Used 1 tool')).toBeTruthy();
    const sent = calls.find((c) => c.url.endsWith('/api/assistant/chat'))?.body as { page: string; messages: { content: string }[] };
    expect(sent.page).toBe('Experiments');
    expect(sent.messages.at(-1)?.content).toBe('How many worlds?');
  });

  it('diff_card_apply_puts', async () => {
    const card = { kind: 'diff', tool: 'scenarios_put', arguments: { id: 'slip', body: { label: 'Renamed' } }, diff: '-label: Old\n+label: Renamed' };
    mockFetch({ reply: 'Here is the change.', cards: [card] });
    await openAndAsk('Rename it');
    fireEvent.click(await screen.findByRole('button', { name: 'Apply' }));
    await waitFor(() => expect(calls.some((c) => c.method === 'PUT')).toBe(true));
    const put = calls.find((c) => c.method === 'PUT');
    expect(put?.url).toMatch(/\/api\/project\/scenarios\/slip$/);
    expect(put?.body).toEqual({ body: { label: 'Renamed' }, confirm: true });
  });

  it('confirm_card_posts_only_on_confirm', async () => {
    const card = { kind: 'confirm', tool: 'experiments_deploy', arguments: { id: 'slip-exp' }, access: 'spend' };
    mockFetch({ reply: 'Confirm to start.', cards: [card] });
    await openAndAsk('Run it');
    expect(await screen.findByText('Needs your OK')).toBeTruthy();
    expect(calls.some((c) => c.url.endsWith('/deploy'))).toBe(false);
    fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    await waitFor(() => expect(calls.some((c) => c.url.endsWith('/api/project/experiments/slip-exp/deploy') && c.method === 'POST')).toBe(true));
  });

  it('drawer_without_project', async () => {
    mockFetch({ reply: 'No project is open yet. Start in Get started.' });
    await openAndAsk('Hello');
    expect(await screen.findByText(/No project is open yet/)).toBeTruthy();
  });

  it('delete_card_shows_what_uses_the_item_and_lets_the_person_choose', async () => {
    const card = { kind: 'confirm', tool: 'worlds_delete', arguments: { id: 'mill' }, access: 'delete' };
    mockFetch({ reply: 'Confirm to delete.', cards: [card] });
    await openAndAsk('Delete the mill');
    expect(await screen.findByText('Delete world mill')).toBeTruthy();
    expect(await screen.findByText('Belt jam')).toBeTruthy();
    expect(screen.queryByText(/"id"/)).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Delete with 1 scenario' }));
    await waitFor(() => expect(calls.some((c) => c.method === 'DELETE')).toBe(true));
    expect(calls.find((c) => c.method === 'DELETE')?.body).toEqual({ cascade: true, confirm: true, expect: DEPS });
    expect(await screen.findByText('Done.')).toBeTruthy();
  });

  it('a_non_put_diff_card_applies_its_own_tool_and_lists_refresh', async () => {
    const card = { kind: 'diff', tool: 'worlds_rename_group', arguments: { from: 'Rebolt', to: 'Rebolt Foods' }, diff: '-group: Rebolt\n+group: Rebolt Foods' };
    mockFetch({ reply: 'Here is the change.', cards: [card] });
    const heard = vi.fn();
    window.addEventListener(PROJECT_CHANGED, heard);
    await openAndAsk('Rename the group');
    expect(await screen.findByText('Rename world group')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Apply' }));
    await waitFor(() => expect(calls.some((c) => c.method === 'POST' && c.url.endsWith('/api/project/world-groups/rename'))).toBe(true));
    expect(calls.find((c) => c.url.endsWith('/rename'))?.body).toEqual({ from: 'Rebolt', to: 'Rebolt Foods', confirm: true });
    await waitFor(() => expect(heard).toHaveBeenCalled());
    window.removeEventListener(PROJECT_CHANGED, heard);
  });

  it('a_settled_card_stays_settled_when_the_drawer_reopens', async () => {
    const card = { kind: 'confirm', tool: 'worlds_delete', arguments: { id: 'mill' }, access: 'delete' };
    mockFetch({ reply: 'Confirm to delete.', cards: [card] });
    await openAndAsk('Delete the mill');
    fireEvent.click(await screen.findByRole('button', { name: 'Delete with 1 scenario' }));
    expect(await screen.findByText('Done.')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Close' }));
    const asked = calls.filter((c) => c.url.endsWith('/dependents')).length;
    act(() => { window.dispatchEvent(new CustomEvent(ASK_EVENT)); });
    expect(await screen.findByText('Done.')).toBeTruthy();
    expect(calls.filter((c) => c.url.endsWith('/dependents')).length).toBe(asked);
  });
});
