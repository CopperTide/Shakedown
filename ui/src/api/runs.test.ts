import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Team } from '@/model/types';
import { createExperiment } from './experiments';
import { startRun } from './runs';

afterEach(() => vi.unstubAllGlobals());

/** A fetch that answers `{}` and remembers the bodies it was sent. */
function bodies() {
  const sent: unknown[] = [];
  vi.stubGlobal('fetch', vi.fn(async (_url: string, init?: RequestInit) => { sent.push(JSON.parse(String(init?.body))); return new Response('{}'); }));
  return sent;
}

const team: Team = { id: 'a', name: 'Team a', agents: [] };

describe('the world a run or an experiment is made on', () => {
  it('a run is asked for on the page’s benchmark, and on the current one when the page names none', async () => {
    const sent = bodies();
    await startRun('t1', team, { runner: 'concurrent', benchmark: 'acme-tools' });
    await startRun('t1', team);
    expect(sent[0]).toEqual({ task_id: 't1', team, runner: 'concurrent', benchmark: 'acme-tools' });
    expect(sent[1]).toEqual({ task_id: 't1', team, runner: 'chain' });
  });
  it('a run with a Code connection is external, on the page’s benchmark', async () => {
    const sent = bodies();
    await startRun('t1', team, { runner: 'chain', connection: 'k1', benchmark: 'acme-tools' });
    expect(sent[0]).toEqual({ task_id: 't1', team, runner: 'external', connection: 'k1', benchmark: 'acme-tools' });
  });
  it('an experiment is asked for on the page’s benchmark', async () => {
    const sent = bodies();
    await createExperiment({ benchmark: 'acme-tools', task: 't1', runs: 2, setups: [{ name: 'Setup 1', team, runner: 'chain' }] });
    expect(sent[0]).toMatchObject({ benchmark: 'acme-tools', task: 't1' });
  });
});
