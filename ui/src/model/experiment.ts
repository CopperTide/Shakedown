// Mirrors shaked's experiments API (`api/experiments.rs`): an experiment is one scenario run by
// 1–4 agent setups, N times each; each setup is stored as a suite, so its report is a SetupReport.
import type { ExperimentStatus, SetupReport } from './report';
import type { ScenarioChoice } from './scenario';
import type { Runner, Team } from './types';

/** A setup's line in the list: its suite, name, team and how many of its runs ended and passed. */
export interface SetupLine {
  suite_id: string;
  name: string;
  team_name: string;
  runner: Runner;
  runs: number;
  ended: number;
  passed: number;
}

interface ExperimentBase {
  id: string;
  name: string;
  /** The scenario's task (a suite from before experiments may have several). */
  tasks: string[];
  benchmark: string;
  /** Runs per setup. */
  runs: number;
  created_at: string;
  status: ExperimentStatus;
  /** A suite from before experiments, shown as a one-setup experiment. */
  legacy: boolean;
  /** The seed the runs' seeds are made from. */
  seed?: number;
  /** Each run's world time window in minutes; null without one. */
  world_minutes?: number | null;
  /** The spend cap per run in USD; null without one. */
  max_cost_usd?: number | null;
}

export interface ListedExperiment extends ExperimentBase {
  setups: SetupLine[];
}

export interface Experiment extends ExperimentBase {
  ended_at: string | null;
  error: string | null;
  setups: SetupReport[];
}

export interface NewSetup {
  name: string;
  team: Team;
  runner: Runner;
  /** A Code connection whose driver runs the setup (its runner is then `external`). */
  connection?: string;
}

export interface NewExperiment {
  name?: string;
  /** The benchmark its runs are made on (the page's); absent, the current one. */
  benchmark?: string;
  task: string;
  scenario?: ScenarioChoice;
  runs: number;
  setups: NewSetup[];
}
