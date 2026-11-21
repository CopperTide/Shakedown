// Import a team from a file: a CopperTide design (`TeamDraft`: groups, agents with a job and a
// place on the board, "hands work to" connections), or an Shakedown team. Every agent stays a real
// model: Claude agents become Claude Code, Codex agents Codex; a kind Shakedown can't run is refused
// by name, never swapped for something else.
import { type JsonObject, asRecord } from '@/lib/record';
import type { AgentKind, AgentSpec, Team } from '@/model/types';
import type { Board, BoardLink } from './board';

export type Imported = { team: Team; from: 'Sculptured' | 'Shakedown' } | { error: string };

type Json = JsonObject;
/** A file's object fields; an array where an object belongs reads as empty. */
const obj = (v: unknown): Json => (Array.isArray(v) ? {} : asRecord(v));
const str = (v: unknown): string | null => (typeof v === 'string' && v.trim() ? v.trim() : null);
const num = (v: unknown) => (typeof v === 'number' && Number.isFinite(v) ? v : 0);
const list = (v: unknown): unknown[] => (Array.isArray(v) ? v : []);

const KIND: Record<string, AgentKind> = { claude: 'claude_code', codex: 'codex' };
const SCULPTURED_KIND_NAME: Record<string, string> = { opencode: 'OpenCode' };
const SHAKEDOWN_KINDS: AgentKind[] = ['claude_code', 'codex', 'open', 'external'];

/** A CopperTide agent id as an Shakedown one (`^[a-z0-9_-]+$`), unique in `taken`. */
function shakedownId(id: string, taken: Set<string>): string {
  const base = id.toLowerCase().replace(/[^a-z0-9_-]+/g, '-').replace(/^-+|-+$/g, '') || 'agent';
  let out = base;
  for (let n = 2; taken.has(out); n++) out = `${base}-${n}`;
  taken.add(out);
  return out;
}

/** The order the agents run in one after another: each after the agents that hand it work, otherwise as the file lists them. */
export function workOrder<T extends { id: string }>(agents: T[], links: BoardLink[]): T[] {
  const left = [...agents];
  const out: T[] = [];
  while (left.length) {
    const done = new Set(out.map((a) => a.id));
    const waiting = new Set(links.filter((l) => !done.has(l.from) && left.some((a) => a.id === l.from)).map((l) => l.to));
    // A cycle leaves no agent free: take the first in file order.
    const next = left.findIndex((a) => !waiting.has(a.id));
    out.push(...left.splice(Math.max(0, next), 1));
  }
  return out;
}

/** The model an agent of `kind` runs on: its own, else its group's, else the team's (none: the CLI's default). */
function modelFor(a: Json, kind: string, groups: Map<string, Json>, team: Json): string | null {
  const own = str(obj(a.advanced).model);
  const group = str(obj(obj(groups.get(String(a.group_id))).models)[kind]);
  return own ?? group ?? str(obj(team.models)[kind]);
}

function fromSculptured(d: Json): Imported {
  const raw = list(d.agents).map(obj);
  const unsupported = raw.filter((a) => !KIND[String(a.kind)]);
  if (unsupported.length) {
    const kinds = [...new Set(unsupported.map((a) => SCULPTURED_KIND_NAME[String(a.kind)] ?? String(a.kind)))].join(', ');
    return { error: `Shakedown can't run ${kinds} agents yet: ${unsupported.map((a) => str(a.name) ?? str(a.id)).join(', ')}. Change them to Claude or Codex in CopperTide and export again.` };
  }
  const groups = new Map(list(d.groups).map(obj).map((g) => [String(g.id), g]));
  const taken = new Set<string>();
  const ids = new Map(raw.map((a) => [String(a.id), shakedownId(String(a.id), taken)]));
  const links = list(d.connections).map(obj)
    .filter((c) => c.kind === 'hands-work-to' && ids.has(String(c.from)) && ids.has(String(c.to)))
    .map((c) => ({ from: ids.get(String(c.from))!, to: ids.get(String(c.to))! }));
  const agents: AgentSpec[] = raw.map((a) => {
    const kind = String(a.kind);
    const writes = obj(obj(a.advanced).data_access).writes;
    return {
      id: ids.get(String(a.id))!,
      name: str(a.name) ?? String(a.id),
      kind: KIND[kind],
      model: modelFor(a, kind, groups, d),
      prompt: typeof a.job === 'string' ? a.job : '',
      // An agent whose design says it writes nothing gets only the world's read tools.
      read_only: Array.isArray(writes) && writes.length === 0,
    };
  });
  const decides = Object.fromEntries(raw.map((a) => [ids.get(String(a.id))!, list(obj(obj(a.advanced).authority).domains).filter((x): x is string => typeof x === 'string')]).filter(([, d]) => d.length));
  const board: Board = {
    groups: [...groups.values()].map((g) => ({ id: String(g.id), label: str(g.label) ?? String(g.id), x: num(g.x), y: num(g.y), w: num(g.w), h: num(g.h) })),
    at: Object.fromEntries(raw.map((a) => [ids.get(String(a.id))!, { x: num(a.x), y: num(a.y) }])),
    links,
    ...(Object.keys(decides).length ? { decides } : {}),
  };
  const team: Team = { id: shakedownId(str(d.id) ?? 'sculptured-team', new Set()), name: str(d.name) ?? 'Imported team', agents: workOrder(agents, links), board };
  return { team, from: 'Sculptured' };
}

/** True when an agent has what Shakedown runs it with: an id, a name, a kind it can run and a prompt. */
const isAgent = (a: Json) => !!str(a.id) && !!str(a.name) && SHAKEDOWN_KINDS.includes(a.kind as AgentKind) && typeof a.prompt === 'string';

/** True when the file is a Shakedown team as it is: an id, a name and agents Shakedown can run. */
const isTeam = (d: Json): d is Json & Team =>
  typeof d.id === 'string' && typeof d.name === 'string' && list(d.agents).every((a) => isAgent(obj(a)));

function fromShakedown(d: Json): Imported {
  if (isTeam(d)) return { team: d, from: 'Shakedown' };
  const bad = list(d.agents).map(obj).find((a) => !isAgent(a));
  return { error: `The agent ${str(bad?.name) ?? str(bad?.id) ?? '(unnamed)'} needs an id, a name, a kind (claude_code, codex or open) and a prompt.` };
}

/** A team from a file's text, or what is wrong with it. */
export function importTeam(text: string): Imported {
  let d: Json;
  try {
    d = obj(JSON.parse(text));
  } catch {
    return { error: "This isn't a JSON file. Import a CopperTide design (.json) or a Shakedown team." };
  }
  if ('madl' in d) return { error: 'This is a MADL file. Open it in CopperTide and import the team design CopperTide saves instead.' };
  const agents = list(d.agents).map(obj);
  if (!agents.length) return { error: 'There are no agents in this file.' };
  if (agents.some((a) => 'job' in a) && 'connections' in d) return fromSculptured(d);
  if (str(d.id) && str(d.name)) return fromShakedown(d);
  return { error: 'This file is neither a CopperTide design nor a Shakedown team.' };
}
