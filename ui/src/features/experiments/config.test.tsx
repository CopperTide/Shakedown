// @vitest-environment jsdom
// An experiment's configuration, folded under its results: seed, world time window, spend cap and
// each setup's runner and agents.
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { Experiment } from '@/model/experiment';
import type { SetupReport } from '@/model/report';
import ExperimentConfig from './ExperimentConfig';

afterEach(cleanup);

const setup = { id: 's1', name: 'Solo planner', runner: 'dispatch', team: { id: 't', name: 'T', agents: [{ id: 'lead', name: 'lead', kind: 'open', model: 'deepseek-v4', prompt: '' }] } } as unknown as SetupReport;
const experiment = (over: Partial<Experiment> = {}): Experiment => ({
  id: 'e1', name: 'Storm', tasks: ['t'], benchmark: 'airline', runs: 3, created_at: '2026-11-21T10:00:00Z', status: 'finished', legacy: false,
  ended_at: null, error: null, setups: [setup], seed: 42, world_minutes: 90, max_cost_usd: 5, ...over,
});

describe('experiment configuration', () => {
  it('is folded and holds the seed, window, cap and agents', () => {
    render(<ExperimentConfig list={[experiment()]} />);
    expect(screen.getByText('Configuration').closest('details')!.open).toBe(false);
    for (const t of ['42', '90 min', '$5.00', 'Solo planner', 'a lead dispatches · lead (deepseek-v4)']) expect(screen.getByText(t)).toBeTruthy();
  });

  it('says when there is no window or cap', () => {
    render(<ExperimentConfig list={[experiment({ world_minutes: null, max_cost_usd: null })]} />);
    expect(screen.getByText('None')).toBeTruthy();
    expect(screen.getByText('No cap')).toBeTruthy();
  });
});
