// A state as a glyph and a word; `abnormal` is the only thing that adds colour (ISA-101).
import type { ExperimentStatus } from '@/model/report';
import type { BranchStatus } from '@/model/types';

export interface Mark {
  glyph: string;
  word: string;
  abnormal: boolean;
}

/** A mark from its parts; plain unless `abnormal`. */
export const mark = (glyph: string, word: string, abnormal = false): Mark => ({ glyph, word, abnormal });

/** The marks the pages share. */
export const MARK = {
  starting: mark('◔', 'Starting'),
  running: mark('◐', 'Running'),
  queued: mark('◔', 'Queued'),
  paused: mark('‖', 'Paused'),
  passed: mark('✓', 'Passed'),
  failed: mark('✕', 'Failed', true),
  finished: mark('■', 'Finished'),
  stopped: mark('■', 'Stopped'),
  interrupted: mark('■', 'Interrupted', true),
  infraError: mark('⚠', 'Infra error', true),
} as const;

/** ✓ when it passed, ✕ (abnormal) when not, each with its word. */
export const passMark = (ok: boolean, [pass, fail]: [string, string] = ['Passed', 'Failed']): Mark =>
  ok ? mark('✓', pass) : mark('✕', fail, true);

/** ⚠ and a word: something went wrong (a refusal, an error). */
export const alarmMark = (word: string): Mark => mark('⚠', word, true);

const RUN: Record<BranchStatus, Mark> = {
  running: MARK.running,
  queued: MARK.queued,
  passed: MARK.passed,
  failed: MARK.failed,
  infra_error: MARK.infraError,
  interrupted: MARK.interrupted,
  stopped: MARK.stopped,
};

/** A run's status. */
export const runMark = (status: BranchStatus): Mark => RUN[status] ?? mark('?', status);

const EXPERIMENT: Record<ExperimentStatus, Mark> = {
  running: MARK.running,
  finished: MARK.finished,
  stopped: MARK.stopped,
};

/** An experiment's status (a stopped one ended early, by choice or because shaked stopped). */
export const experimentMark = (s: ExperimentStatus): Mark => EXPERIMENT[s];
