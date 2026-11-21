// Mirrors shaked's connections API (`api/connections.rs`) and the agent check (`POST /api/agents/check`).
import type { AgentSpec, BranchStatus } from './types';

export type CheckBody =
  | { kind: 'http'; url: string; headers?: Record<string, string> }
  | { kind: 'bedrock'; model_id: string; region: string };

/** shaked's answer to the check: the agent as it will store it (header values as references), or why not. */
export type CheckAnswer =
  | { ok: true; agent: AgentSpec; name?: string; ms: number }
  | { ok: false; error: string; ms?: number };

export interface Connection {
  id: string;
  name: string;
  kind: 'code';
  command: string[];
  workdir: string | null;
  /** Values of names with KEY, TOKEN, SECRET or PASSWORD read "••••". */
  env: Record<string, string>;
  created_at: string;
}

export interface NewConnection {
  name: string;
  command: string[];
  workdir?: string;
  env: Record<string, string>;
}

/** What a connection test came to (`POST /api/connections/{id}/test`). */
export interface ConnectionTest {
  ok: boolean;
  branch_id: string;
  status: BranchStatus;
  agents: string[];
  tool_calls: number;
  error: string | null;
  exit: string | null;
  stderr_tail: string;
}
