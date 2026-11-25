// AI review, as shaked stores it (`GET/POST /api/branches/{id}/review`, `/api/compare/review`,
// `/api/suites/compare/review`).

export type Severity = 'safety' | 'goal' | 'efficiency';
export type SuggestionKind = 'prompt' | 'team' | 'tool' | 'model' | 'process';

export interface WentWrong {
  title: string;
  why: string;
  steps: number[];
  severity: Severity;
  /** Every step it cited was not in the run, so shaked dropped them. */
  unverified?: boolean;
}

export interface Suggestion {
  change: string;
  why: string;
  kind: SuggestionKind;
  agent?: string;
  /** Sentences to add to the agent's prompt. */
  prompt_text?: string;
  /** The step to rerun from. */
  step?: number;
}

export interface RunReview {
  verdict: string;
  went_wrong: WentWrong[];
  suggestions: Suggestion[];
  went_well: string[];
  confidence: 'high' | 'medium' | 'low' | string;
}

export interface Difference {
  title: string;
  a: string;
  b: string;
  why_it_matters: string;
  /** Steps (two runs) or branch ids of example runs (two suites). */
  steps_a: (number | string)[];
  steps_b: (number | string)[];
  unverified?: boolean;
}

export interface CompareReview {
  headline: string;
  differences: Difference[];
  which_is_better: 'a' | 'b' | 'mixed';
  why: string;
}

export interface StoredReview<R = RunReview | CompareReview> {
  key: string;
  kind: 'run' | 'compare' | 'suites' | 'ask';
  status: 'running' | 'done' | 'error';
  model: string;
  review: R | null;
  issues: string[];
  error: string | null;
  cost_usd: number | null;
  cost_source: string | null;
  tokens: number | null;
  seconds: number | null;
  digest_bytes: number | null;
  created_at: string;
  finished_at: string | null;
}

export interface ReviewSettings {
  enabled: boolean;
  model: string;
  why?: string;
  /** Asking about a step is on (it runs on the asker's own key; off only by the switch). */
  ask?: boolean;
}

/** A quote of what the agent had at a step, checked word for word against the record. */
export interface Evidence {
  /** The source's id: `s<seq>`, `m<seq>.<i>` (a message sent in model call `seq`), `ticket`, `prompt`. */
  source: string;
  step: number | null;
  /** Where in the record: `request.messages[0] (system)`, `response`, `tool_call by ops`. */
  field: string;
  quote: string;
  why: string;
}

/** The explainer's hypothesis, which the record cannot show. */
export interface Guess { text: string; based_on: string[] }

/** A question about a step and, once answered, its checked answer (`GET/POST /api/branches/{id}/questions`). */
export interface StepQuestion {
  question: string;
  step: number;
  agent: string | null;
  /** Who asked (the hosting layer's principal); `mine` when it was the viewer. */
  asked_by: string;
  mine?: boolean;
  answer?: string;
  evidence?: Evidence[];
  speculation?: Guess[];
  sources?: number;
  sources_left_out?: number;
}

/** What a review is of: one branch, two branches, or two setups of an experiment (shaked's suites). */
export type Target =
  | { kind: 'run'; branch: string }
  | { kind: 'compare'; a: string; b: string }
  | { kind: 'setups'; a: string; b: string };
