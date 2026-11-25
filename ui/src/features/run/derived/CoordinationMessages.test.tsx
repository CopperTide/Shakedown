// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import type { Event } from '@/model/types';
import CoordinationMessages from './CoordinationMessages';

const events = [
  { seq: 1, kind: 'delegation', agent: 'lead', payload: { to: 'ops', task: 'Check the delayed flight' } },
  { seq: 2, kind: 'report', agent: 'ops', payload: { to: 'lead', text: 'Aircraft is ready' } },
  { seq: 3, kind: 'message', agent: 'lead', payload: { to: '*', text: 'All clear' } },
  { seq: 4, kind: 'handoff', agent: 'ops', payload: { to: 'crew', note: 'Confirm reserve crew' } },
] as Event[];
afterEach(cleanup);

it('reads each exchange and opens its recorded step', () => {
  const select = vi.fn();
  render(<CoordinationMessages events={events} onSelect={select} />);
  for (const text of ['Check the delayed flight', 'Aircraft is ready', 'All clear', 'Confirm reserve crew']) expect(screen.getByText(text)).toBeTruthy();
  expect(screen.getByText('Everyone', { selector: 'span' })).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'View message at step 2' }));
  expect(select).toHaveBeenCalledWith(2);
});

it('combines participant and content filters', () => {
  render(<CoordinationMessages events={events} onSelect={() => {}} />);
  fireEvent.change(screen.getByLabelText('Messages involving agent'), { target: { value: 'crew' } });
  expect(screen.getAllByRole('article')).toHaveLength(1);
  fireEvent.change(screen.getByLabelText('Search messages'), { target: { value: 'missing' } });
  expect(screen.getByText('No messages match these filters.')).toBeTruthy();
});
