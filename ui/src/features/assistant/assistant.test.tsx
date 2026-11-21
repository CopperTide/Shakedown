// @vitest-environment jsdom
// The Ask drawer: opened from the top bar, sends the page with the question, shows the reply and
// the tools used, and applies a card only when the person clicks.
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ASK_EVENT } from '@/features/shell/TopBar';
import AskDrawer from './AskDrawer';

const calls: { url: string; method: string; body: unknown }[] = [];

function mockFetch(answer: Record<string, unknown>) {
  calls.length = 0;
  vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
    const u = String(url);
    calls.push({ url: u, method: init?.method ?? 'GET', body: init?.body ? JSON.parse(String(init.body)) : null });
    if (u.endsWith('/api/assistant/chat')) return new Response(JSON.stringify({ messages: [], tools: [], cards: [], ...answer }));
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
});
