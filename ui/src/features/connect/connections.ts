// Code connections: a saved command that starts a team's own driver program (LangGraph, CrewAI,
// the OpenAI Agents SDK or anything else), which Shakedown launches on this computer for each run
// that uses it (`/api/connections`). Secret env values come back redacted and stay in arenad.
import { useEffect, useState } from 'react';
import { call, post } from '@/api';
import type { NewSetup } from '@/model/experiment';
import type { BranchStatus, Runner, Team } from '@/model/types';
import type { ConnectForm } from './connect';

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

export const listConnections = () => call<Connection[]>('/api/connections');
export const createConnection = (body: NewConnection) => post<Connection>('/api/connections', body);
export const deleteConnection = (id: string) => call<{ deleted: string }>(`/api/connections/${encodeURIComponent(id)}`, { method: 'DELETE' });
export const testConnection = (id: string) => post<ConnectionTest>(`/api/connections/${encodeURIComponent(id)}/test`, {});

const NAME = /^[A-Za-z_][A-Za-z0-9_]*$/;

/** The env text (`NAME=value` per line, `#` comments) as a map, or the first line that isn't. */
export function parseEnv(text: string): { env: Record<string, string> } | { error: string } {
  const env: Record<string, string> = {};
  const lines = text.split(/\r?\n/);
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i].trim();
    if (!line || line.startsWith('#')) continue;
    const at = line.indexOf('=');
    const name = at > 0 ? line.slice(0, at).trim() : '';
    if (!NAME.test(name)) return { error: `Environment line ${i + 1} isn’t “NAME=value”.` };
    if (name in env) return { error: `The variable ${name} is there twice.` };
    env[name] = line.slice(at + 1).trim();
  }
  return { env };
}

/** A command line as its program and arguments: spaces split, "double" or 'single' quotes keep words together. */
export function splitCommand(line: string): string[] {
  const out: string[] = [];
  let word = '';
  let quote: string | null = null;
  let started = false;
  for (const ch of line) {
    if (quote) {
      if (ch === quote) quote = null; else word += ch;
    } else if (ch === '"' || ch === "'") {
      quote = ch; started = true;
    } else if (/\s/.test(ch)) {
      if (started) { out.push(word); word = ''; started = false; }
    } else {
      word += ch; started = true;
    }
  }
  if (started) out.push(word);
  return out;
}

/** Why a Code form can't be saved and tested yet, or null. */
export function codeProblem(f: ConnectForm): string | null {
  if (!splitCommand(f.command).length) return 'Give the command that starts your driver.';
  if (!f.name.trim()) return 'Name the connection.';
  const env = parseEnv(f.env);
  return 'error' in env ? env.error : null;
}

/** The connection a Code form saves; null while it has a problem. */
export function codeBody(f: ConnectForm): NewConnection | null {
  if (codeProblem(f)) return null;
  const env = parseEnv(f.env);
  const body: NewConnection = { name: f.name.trim(), command: splitCommand(f.command), env: 'env' in env ? env.env : {} };
  return f.workdir.trim() ? { ...body, workdir: f.workdir.trim() } : body;
}

/** A connection as a team to pick: no agents of its own (its driver registers them), run by its driver. */
export function codeTeam(c: Connection): Team {
  return { id: `code-${c.id}`, name: c.name, agents: [], runner: 'external', connection: c.id };
}

/** The saved Code connections as teams to pick, with `first` (one just made) among them; none while arenad can't say. */
export function useCodeTeams(first?: Team): Team[] {
  const [teams, setTeams] = useState<Team[]>(() => (first?.connection ? [first] : []));
  useEffect(() => {
    let live = true;
    listConnections().then((l) => {
      if (!live) return;
      const saved = l.map(codeTeam);
      setTeams(first?.connection && !saved.some((t) => t.id === first.id) ? [first, ...saved] : saved);
    }).catch(() => {});
    return () => { live = false; };
  }, [first]);
  return teams;
}

/** The setup an experiment posts for a team: a Code connection's, or the team under its runner. */
export function setupFor(name: string, t: Team, runFor: (t: Team, r: Runner) => Team): NewSetup {
  if (t.connection) return { name, runner: 'external', connection: t.connection, team: { id: t.id, name: t.name, agents: [] } };
  const runner = t.runner ?? 'chain';
  return { name, team: runFor(t, runner), runner };
}

const count = (n: number, one: string) => `${n} ${one}${n === 1 ? '' : 's'}`;

/** "2 agents · 7 tool calls · passed". */
export function testLine(t: ConnectionTest): string {
  return `${count(t.agents.length, 'agent')} · ${count(t.tool_calls, 'tool call')} · ${t.status.replace('_', ' ')}`;
}
