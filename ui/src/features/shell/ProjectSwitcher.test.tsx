// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { ProjectSwitcher } from './ProjectSwitcher';

const calls = vi.hoisted(() => ({ select: vi.fn(), create: vi.fn() }));
vi.mock('@/api/projects', () => ({
  fetchProjects: async () => ({ current: 'a', projects: [{ id: 'a', name: 'Alpha' }, { id: 'b', name: 'Beta' }] }),
  selectProject: calls.select, createProject: calls.create,
}));
afterEach(() => { cleanup(); vi.clearAllMocks(); });

async function menu() {
  render(<ProjectSwitcher name="Alpha" />);
  fireEvent.keyDown(screen.getByRole('button', { name: 'Project: Alpha' }), { key: 'ArrowDown' });
  await screen.findByRole('menuitem', { name: 'Beta' });
}

it('shows a failed switch without leaving the project', async () => {
  calls.select.mockRejectedValue(new Error('Project unavailable'));
  await menu();
  fireEvent.click(screen.getByRole('menuitem', { name: 'Beta' }));
  expect((await screen.findByRole('alert')).textContent).toContain('Project unavailable');
  expect(calls.select).toHaveBeenCalledWith('b');
});

it('creates a named project and keeps a rejected name editable', async () => {
  calls.create.mockRejectedValue(new Error('That name is already used'));
  await menu();
  fireEvent.click(screen.getByRole('menuitem', { name: 'New project' }));
  fireEvent.change(await screen.findByLabelText('Project name'), { target: { value: '  Beta  ' } });
  fireEvent.click(screen.getByRole('button', { name: 'Create project' }));
  await waitFor(() => expect(calls.create).toHaveBeenCalledWith('Beta'));
  expect((await screen.findByRole('alert')).textContent).toContain('That name is already used');
});
