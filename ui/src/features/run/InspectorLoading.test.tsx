// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { branchEvent } from '@/api/branches';
import type { Event } from '@/model/types';
import Inspector from './Inspector';

vi.mock('@/api/branches', () => ({ branchEvent: vi.fn(), rows: vi.fn() }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });
const call = (seq: number): Event => ({ branch_id: 'b', seq, at: '2026-11-25T12:00:00Z', agent: 'agent', kind: 'model_call',
  payload: { bodies_omitted: true, request: { model: "test-model" }, response: null, usage: {}, cost_usd: 0, cost_source: null } });
const full = (seq: number): Event => ({ ...call(seq), kind: 'model_call', payload: {
  request: { messages: [{ role: 'user', content: `request ${seq}` }] }, response: `response ${seq}`, usage: {}, cost_usd: 0, cost_source: null,
} });

it('loads only the selected model body and ignores a stale selection response', async () => {
  let finishFirst!: (e: Event) => void;
  vi.mocked(branchEvent).mockImplementationOnce(() => new Promise((resolve) => { finishFirst = resolve; })).mockResolvedValueOnce(full(2));
  const { rerender } = render(<Inspector branchId="b" event={call(1)} />);
  expect(screen.getByText('Loading this model call’s details…')).toBeTruthy();
  rerender(<Inspector branchId="b" event={call(2)} />);
  await screen.findByText(/response 2/);
  finishFirst(full(1));
  await waitFor(() => expect(screen.queryByText(/response 1/)).toBeNull());
  expect(branchEvent).toHaveBeenNthCalledWith(1, 'b', 1);
  expect(branchEvent).toHaveBeenNthCalledWith(2, 'b', 2);
});

it('retries a failed body load without losing the step and does not refetch full events', async () => {
  vi.mocked(branchEvent).mockRejectedValueOnce(new Error('offline')).mockResolvedValueOnce(full(1));
  const { rerender } = render(<Inspector branchId="b" event={call(1)} />);
  await screen.findByRole('alert');
  fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
  await screen.findByText(/response 1/);
  rerender(<Inspector branchId="b" event={full(3)} />);
  expect(screen.getByText(/response 3/)).toBeTruthy();
  expect(branchEvent).toHaveBeenCalledTimes(2);
});
