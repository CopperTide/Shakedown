import { describe, expect, it } from 'vitest';
import { payloadOf, runStartOf } from './events';
import type { Event } from './types';

const ev = (seq: number, kind: Event['kind'], payload: unknown): Event => ({ branch_id: 'b', seq, at: '2026-11-19T00:00:00Z', agent: null, kind, payload } as Event);

describe('typed payloads', () => {
  it('a payload is read for its own kind only', () => {
    const call = ev(2, 'tool_call', { name: 'find_user', args: {} });
    expect(payloadOf(call, 'tool_call')?.name).toBe('find_user');
    expect(payloadOf(call, 'check')).toBeNull();
    expect(payloadOf(undefined, 'check')).toBeNull();
  });
  it('the run start, including an old one with only its task', () => {
    expect(runStartOf([ev(1, 'run_start', { task_id: '27' }), ev(2, 'agent_start', {})])?.task_id).toBe('27');
    expect(runStartOf([ev(2, 'agent_start', {})])).toBeUndefined();
  });
});
