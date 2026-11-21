// The kinds of team a setup can have, and the empty team each starts from.
import type { TeamSpec } from '@/model/project';

export type TeamKind = TeamSpec['kind'];

export const TEAM_KINDS: { kind: TeamKind; label: string; about: string }[] = [
  { kind: 'builtin', label: 'Built-in agents', about: 'You pick roles, prompts and models.' },
  { kind: 'code', label: 'Your code', about: 'A command that runs your agents, e.g. a LangGraph or CrewAI script.' },
  { kind: 'http', label: 'HTTP endpoint', about: 'An agent already deployed, e.g. on Bedrock.' },
  { kind: 'claude-code', label: 'Claude Code', about: 'Headless, with the world\'s tools over MCP.' },
  { kind: 'codex', label: 'Codex', about: 'Headless, with the world\'s tools over MCP.' },
  { kind: 'madl', label: 'Described in MADL', about: 'A MADL team file: the full graph, checks and coordination view.' },
];

/** The empty team of `kind`. */
export function teamFor(kind: TeamKind): TeamSpec {
  switch (kind) {
    case 'builtin': return { kind, agents: [] };
    case 'code': return { kind, command: '' };
    case 'http': return { kind, url: '' };
    case 'madl': return { kind, file: 'team.madl.yaml' };
    default: return { kind };
  }
}
