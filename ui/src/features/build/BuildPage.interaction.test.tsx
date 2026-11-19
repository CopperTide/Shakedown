// @vitest-environment jsdom
// The builder page driven as a person does (typing, clicking Next and Back), over a mocked builder API.
import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '@/api/client';
import BuildPage from './BuildPage';
import { built, form, people, preview, templates } from './fixtures';

const api = vi.hoisted(() => ({
  templates: vi.fn(), templateForm: vi.fn(), previewWorld: vi.fn(), buildWorld: vi.fn(), worldState: vi.fn(), prepareWorld: vi.fn(),
}));
vi.mock('@/api/worlds', () => api);

/** A request that is still on its way. */
const pending = () => new Promise<never>(() => {});
const other = { ...form, id: 'other', name: 'Other example', company: { name: 'Other Co' } };

beforeEach(() => {
  Object.values(api).forEach((f) => f.mockReset());
  api.templates.mockResolvedValue(templates);
  api.templateForm.mockImplementation(async (id: string) => (id === 'other' ? other : form));
  api.previewWorld.mockResolvedValue(preview);
});
afterEach(cleanup);

async function open() {
  const user = userEvent.setup();
  render(<MemoryRouter><BuildPage /></MemoryRouter>);
  await screen.findByText('Other example');
  return user;
}

/** Clicks the bar's main button (Next, or Build world) once it is enabled, `times` times. */
async function forward(user: ReturnType<typeof userEvent.setup>, times = 1, name = 'Next') {
  for (let i = 0; i < times; i++) {
    const button = () => screen.getByRole<HTMLButtonElement>('button', { name });
    await waitFor(() => expect(button().disabled).toBe(false));
    await user.click(button());
  }
}

const wait = (ms: number) => act(() => new Promise((r) => setTimeout(r, ms)));

describe('the builder page, used', () => {
  it('keeps a name being typed, and its focus, while the next preview is asked for', async () => {
    api.previewWorld.mockResolvedValueOnce({ ...preview, people: people(3) }).mockImplementation(pending);
    const user = await open();
    await forward(user, 3);
    const [input] = await screen.findAllByLabelText<HTMLInputElement>('Name');
    await user.type(input, 'xyz');
    await wait(300);
    expect(api.previewWorld).toHaveBeenCalledTimes(2);
    expect(screen.getAllByLabelText('Name')[0]).toBe(input);
    expect(document.activeElement).toBe(input);
    expect(input.value).toBe('Ada Examplexyz');
  });

  it('keeps a rename while a preview with problems comes back, as when a count is cleared', async () => {
    const user = await open();
    await forward(user, 3);
    const [input] = await screen.findAllByLabelText<HTMLInputElement>('Name');
    await user.clear(input);
    await user.type(input, 'Bo Lee');
    api.previewWorld.mockResolvedValue({ ...preview, people: [], tasks: [], events: [], problems: [{ path: 'people.counts.dev', message: 'Developers need at least 1' }] });
    await user.clear(screen.getByLabelText('Developers'));
    await wait(400);
    api.previewWorld.mockResolvedValue(preview);
    await user.type(screen.getByLabelText('Developers'), '8');
    await wait(400);
    expect(screen.getAllByLabelText<HTMLInputElement>('Name')[0].value).toBe('Bo Lee');
    expect(api.previewWorld.mock.calls.at(-1)?.[1].people.edits).toEqual({ 'dev-001': { name: 'Bo Lee' } });
  });

  it('choosing another template starts from its own defaults', async () => {
    const user = await open();
    await forward(user);
    const company = screen.getByLabelText<HTMLInputElement>('Company name');
    await user.clear(company);
    await user.type(company, 'Mine');
    await user.click(screen.getByRole('button', { name: 'Back' }));
    await user.click(screen.getByText('Other example'));
    await forward(user);
    expect(screen.getByLabelText<HTMLInputElement>('Company name').value).toBe('Other Co');
  });

  it('says why the world could not be checked instead of waiting for ever', async () => {
    api.previewWorld.mockRejectedValue(new ApiError(502, 'the world builder is not running'));
    const user = await open();
    await forward(user, 5);
    expect((await screen.findAllByText(/the world builder is not running/)).length).toBeGreaterThan(0);
    expect(screen.queryByText(/Checking the world/)).toBeNull();
  });

  it('a refused build says why until the step changes', async () => {
    api.buildWorld.mockRejectedValue(new ApiError(422, 'refused', { problems: [{ path: '', message: 'the name is taken' }] }));
    const user = await open();
    await forward(user, 5);
    await forward(user, 1, 'Build world');
    expect(await screen.findByText('⚠ The name is taken')).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Back' }));
    expect(screen.queryByText(/The name is taken/)).toBeNull();
  });

  it('a world left unprepared offers Try again, and a preparation already under way is waited for', async () => {
    api.buildWorld.mockResolvedValue({ name: built.name, state: 'preparing' });
    api.worldState.mockResolvedValueOnce({ ...built, state: 'not_prepared' }).mockResolvedValue({ ...built, state: 'ready', elapsed_ms: 1000 });
    api.prepareWorld.mockRejectedValue(new ApiError(409, `${built.name} is being prepared`));
    const user = await open();
    await forward(user, 5);
    await forward(user, 1, 'Build world');
    await user.click(await screen.findByRole('button', { name: 'Try again' }));
    expect(await screen.findByText(/✓ Ready/)).toBeTruthy();
    expect(api.prepareWorld).toHaveBeenCalledWith(built.name);
  });
});
