// @vitest-environment jsdom
// Get started: each step ticks itself off from live state, nothing is preselected, a project is
// created only on a click, git is offered only for a folder, there is no skipping, and the
// assistant opens with a question already written.
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';
import AskDrawer from '@/features/assistant/AskDrawer';
import { ASK_EVENT } from '@/features/shell/TopBar';
import { forgetShared } from '@/hooks/query';
import GetStartedPage from './GetStartedPage';
import { stepStates } from './steps';

const posts: { url: string; body: unknown }[] = [];

function mockFetch(project: { status: number; body: unknown }, providers: unknown[] = []) {
  posts.length = 0;
  forgetShared();
  vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
    const u = String(url);
    if (init?.method === 'POST') { posts.push({ url: u, body: JSON.parse(String(init.body)) }); return new Response(JSON.stringify({ name: 'New', counts: {}, problems: [] })); }
    if (u.endsWith('/api/project')) return new Response(JSON.stringify(project.body), { status: project.status });
    if (u.endsWith('/api/providers')) return new Response(JSON.stringify(providers));
    if (u.includes('/api/project/')) return new Response(JSON.stringify({ items: [] }));
    return new Response(JSON.stringify({ error: 'not found' }), { status: 404 });
  }));
}

const NOT_A_PROJECT = { status: 409, body: { error: 'no project is open: this folder is not a Shakedown project (no shakedown.yaml)', folder: true } };
const NOT_IN_DB = { status: 409, body: { error: 'no project is open: this folder is not a Shakedown project (no shakedown.yaml)', folder: false } };
const NOT_CONFIGURED = { status: 409, body: { error: 'no project is open: start shaked with SHAKEDOWN_PROJECT set to a project folder' } };
const OPEN = { status: 200, body: { name: 'Larkspur Ops', counts: { worlds: 1, scenarios: 0, setups: 2, experiments: 0 }, problems: [] } };

const show = () => render(<MemoryRouter><GetStartedPage /></MemoryRouter>);

afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe('get started', () => {
  it('step_states_from_live_state', () => {
    const states = stepStates({ project: 'open', counts: { worlds: 1, scenarios: 0, setups: 2, experiments: 0 }, providersOk: false });
    expect(states.map((s) => [s.id, s.done])).toEqual([['project', true], ['model', false], ['world', true], ['agents', true], ['scenario', false], ['ready', false]]);
  });

  it('not_configured_explains', async () => {
    mockFetch(NOT_CONFIGURED);
    show();
    expect(await screen.findByText(/SHAKEDOWN_PROJECT/)).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Create project' })).toBeNull();
  });

  it('existing_project_is_done', async () => {
    mockFetch(OPEN);
    show();
    expect(await screen.findByText('Larkspur Ops is ready.')).toBeTruthy();
  });

  it('nothing_preselected', async () => {
    mockFetch(NOT_A_PROJECT);
    show();
    const name = await screen.findByLabelText('Project name');
    expect((name as HTMLInputElement).value).toBe('');
    expect(screen.getByRole('switch', { name: 'Track with git' }).getAttribute('aria-checked')).toBe('false');
  });

  it('git_hidden_when_not_a_folder', async () => {
    mockFetch(NOT_IN_DB);
    show();
    fireEvent.change(await screen.findByLabelText('Project name'), { target: { value: 'Larkspur Ops' } });
    expect(screen.queryByRole('switch', { name: 'Track with git' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Create project' }));
    await waitFor(() => expect(posts.length).toBe(1));
    expect(posts[0].body).toEqual({ name: 'Larkspur Ops', git: false });
  });

  it('no_skip', async () => {
    mockFetch(NOT_A_PROJECT);
    show();
    await screen.findByLabelText('Project name');
    expect(screen.queryByRole('link', { name: /skip/i })).toBeNull();
  });

  it('ready_waits_for_every_step', async () => {
    mockFetch(OPEN);
    show();
    fireEvent.click(await screen.findByRole('button', { name: /Ready/ }));
    expect(screen.getByText('Finish the steps above to open Shakedown.')).toBeTruthy();
    expect(screen.queryByRole('link', { name: 'Create an experiment' })).toBeNull();
  });

  it('create_project_posts', async () => {
    mockFetch(NOT_A_PROJECT);
    show();
    fireEvent.change(await screen.findByLabelText('Project name'), { target: { value: 'Larkspur Ops' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create project' }));
    await waitFor(() => expect(posts.length).toBe(1));
    expect(posts[0].body).toEqual({ name: 'Larkspur Ops', git: false });
  });

  it('ask_with_prompt_prefills', async () => {
    mockFetch(OPEN);
    render(<MemoryRouter><AskDrawer /></MemoryRouter>);
    act(() => { window.dispatchEvent(new CustomEvent(ASK_EVENT)); });
    await screen.findByLabelText('Ask the assistant');
    act(() => { window.dispatchEvent(new CustomEvent(ASK_EVENT, { detail: { prompt: 'Draft a world for a valve plant.' } })); });
    expect((screen.getByLabelText('Ask the assistant') as HTMLTextAreaElement).value).toBe('Draft a world for a valve plant.');
  });
});
