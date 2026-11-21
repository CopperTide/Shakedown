import type { AgentSpec, Runner, Team } from './types';

/** What makes two teams the same for `isUntouched`: their agents as shaked runs them. */
const sameAgents = (a: Team, b: Team) =>
  JSON.stringify(a.agents.map(({ id, name, kind, model, prompt, read_only }) => [id, name, kind, model, prompt, !!read_only]))
  === JSON.stringify(b.agents.map(({ id, name, kind, model, prompt, read_only }) => [id, name, kind, model, prompt, !!read_only]));

/**
 * True when `stored` is one of the benchmark's suggested teams as it came, never edited nor
 * picked on purpose: a default the browser kept, which follows the benchmark's new default.
 */
export function isUntouched(stored: Team, suggested: Team[]): boolean {
  return !stored.picked && suggested.some((t) => t.id === stored.id && t.name === stored.name && sameAgents(t, stored));
}

/**
 * The team as a run takes it: an open agent's blank provider dropped; with the `external` runner,
 * every agent is `external` (the system that drives the run runs them, not shaked). The board,
 * the suggested runner and the picked mark stay in the browser.
 */
export function teamForRun({ board: _board, runner: _runner, picked: _picked, connection: _connection, ...team }: Team, runner: Runner = 'chain'): Team {
  return {
    ...team,
    agents: team.agents.map(({ provider, ...a }) => {
      if (runner === 'external') return { ...a, kind: 'external' as const, model: null };
      const p = provider?.trim();
      return a.kind === 'open' && p ? { ...a, provider: p } : a;
    }),
  };
}

/** Agent ids must match ^[a-z0-9_-]+$ and be unique within the team. */
export function slug(name: string, taken: Set<string>): string {
  const base = name.toLowerCase().replace(/[^a-z0-9_-]+/g, '-').replace(/^-+|-+$/g, '') || 'agent';
  let id = base;
  for (let n = 2; taken.has(id); n++) id = `${base}-${n}`;
  return id;
}

/** Re-derive every id from its name (keeping order, resolving collisions). */
export function withSlugIds(agents: AgentSpec[]): AgentSpec[] {
  const taken = new Set<string>();
  return agents.map((a) => {
    const id = slug(a.name, taken);
    taken.add(id);
    return { ...a, id };
  });
}

const MARK = '\n\nDeployment rules:\n';

/** A prompt with the rules in place of any earlier deployment rules. */
export function withRules(prompt: string, rules: string): string {
  const base = prompt.includes(MARK) ? prompt.slice(0, prompt.indexOf(MARK)) : prompt;
  const r = rules.trim();
  return r ? `${base.trimEnd()}${MARK}${r}` : base;
}
