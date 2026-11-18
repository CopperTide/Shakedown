import { describe, expect, it } from 'vitest';
import type { Event, Team } from '../types';
import { stepSummary } from './stepSummary';

const team: Team = { id: 't', name: 'T', agents: [{ id: 'quality', name: 'Quality assurance', kind: 'claude_code', model: 'haiku', prompt: '' }] };
const ev = (seq: number, kind: Event['kind'], payload: unknown, agent: string | null = 'quality'): Event => ({ branch_id: 'b', seq, at: '2026-11-07T12:00:00Z', agent, kind, payload });
const start = ev(1, 'run_start', { systems: { erp: { kind: 'erp', product: 'ERPNext' } }, tools: { erp_insert: { effect: 'write', part: 'erp' }, erp_query: { effect: 'read', part: 'erp' } } }, null);

describe('step summary', () => {
  it('says who did what on which system, and how it went', () => {
    const ok = ev(2, 'tool_call', { name: 'erp_insert', part: 'erp', error: null, duration_ms: 120 });
    expect(stepSummary(ok, [start, ok], team)).toEqual({ title: 'Quality assurance · ERP write', text: 'Quality assurance wrote to the ERP (erp_insert). It worked, in 120 ms.', abnormal: false });
    const refused = ev(3, 'tool_call', { name: 'erp_insert', part: 'erp', error: 'Write tools are not available to this agent: its team made it read-only' });
    expect(stepSummary(refused, [start, refused], team)).toMatchObject({ text: expect.stringContaining('It was refused: Write tools are not available'), abnormal: true });
    const failed = ev(4, 'tool_call', { name: 'erp_query', part: 'erp', error: 'no such table' });
    expect(stepSummary(failed, [start, failed], team).text).toBe('Quality assurance read the ERP (erp_query). It failed: no such table');
  });

  it('puts every other step in a sentence', () => {
    const s = (e: Event) => stepSummary(e, [start, e], team).text;
    expect(s(ev(5, 'message', { to: '*', text: 'Batch held.' }))).toBe('Quality assurance sent a message to everyone: “Batch held.”');
    expect(s(ev(6, 'handoff', { note: 'Done' }))).toBe('Quality assurance left a note for the agents after it: “Done”');
    expect(s(ev(7, 'agent_end', { ok: false, error: 'timed out', seconds: 75 }))).toBe('Quality assurance stopped with an error after 1 min 15 s: timed out');
    expect(s(ev(8, 'check', { passed: false, checks: [{ name: 'a', passed: true }, { name: 'b', passed: false }] }, null))).toBe('The run was scored: 1 of 2 checks passed.');
    expect(s(ev(9, 'resume', { paused_ms: 12_400 }, null))).toBe('The run went on after 12 s paused.');
    expect(s(ev(10, 'model_call', { model: 'claude-haiku', input_tokens: 1200, output_tokens: 80 }))).toBe('Quality assurance asked its model (claude-haiku). 1,200 tokens in, 80 out.');
  });

  it("shows an agent's reasoning, cut to 160 characters", () => {
    const s = (p: unknown) => stepSummary(ev(11, 'reasoning', p), [start], team);
    expect(s({ text: 'Check stock first.', kind: 'thinking', source: 'gateway' })).toEqual({ title: 'Quality assurance · Reasoning', text: 'Check stock first.', abnormal: false });
    expect(s({ text: 'Reserve, then order.', kind: 'plan', source: 'agent' }).title).toBe('Quality assurance · Plan');
    expect(s({ text: 'Reserving now.', kind: 'step', source: 'claude_code' }).title).toBe('Quality assurance · Step');
    const long = s({ text: 'x'.repeat(400), kind: 'thinking', source: 'agent' }).text;
    expect(long).toHaveLength(160);
    expect(long.endsWith('…')).toBe(true);
    const cut = s({ text: 'y'.repeat(50), kind: 'thinking', source: 'agent', truncated: true }).text;
    expect(cut.endsWith('(cut at 32,768 characters)')).toBe(true);
  });
});
