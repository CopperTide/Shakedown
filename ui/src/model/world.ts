// The world builder's shapes, as shaked passes the builder's answers on (docs/protocol.md, "Builder calls") and
// as it lists the benchmarks a user can choose between (`/api/worlds`, docs/reference.md).
import type { ParamSpec } from './scenario';

/** One template, as `GET /api/templates` lists it (`systems` are the systems' labels). */
export interface TemplateSummary {
  id: string;
  name: string;
  summary: string;
  version: string;
  systems: string[];
}

/** A system's option: one of `choices`. */
export interface OptionForm {
  name: string;
  label: string;
  choices: string[];
  default: string;
}

export interface SystemForm {
  id: string;
  label: string;
  product: string;
  summary?: string;
  required: boolean;
  default: boolean;
  /** Other systems chosen with this one. */
  requires: string[];
  memory_mb: number;
  /** Shown on Review, e.g. `code.{company.domain}`. */
  hosts: string[];
  options: OptionForm[];
}

/** A role of simulated people: its count a share of the size (rounded half up) or fixed. */
export interface RoleForm {
  id: string;
  label: string;
  summary?: string;
  count: { share: number } | { fixed: number };
  min: number;
  /** The role is dropped when none of these is chosen. */
  systems: string[];
}

export interface EventForm {
  id: string;
  label: string;
  description: string;
  kind: 'trap' | 'event';
  default: boolean;
  needs: string[];
  params: ParamSpec[];
}

/** `GET /api/templates/{id}`: what the builder's steps offer, with the template's defaults. */
export interface TemplateForm {
  id: string;
  name: string;
  summary: string;
  version: string;
  company: { name: string };
  size: { label: string; default: number; min: number; max: number; presets?: number[] };
  systems: SystemForm[];
  roles: RoleForm[];
  events: EventForm[];
}

/** What the UI sends to preview and build a world (every field optional for the builder). */
export interface Choices {
  seed: number;
  company: { name: string };
  size: number;
  systems: string[];
  options: Record<string, Record<string, string>>;
  people: {
    counts: Record<string, number>;
    edits: Record<string, { name: string }>;
    added: { name: string; role: string }[];
    removed: string[];
  };
  events: Record<string, { on: boolean; params: Record<string, number | string> }>;
}

/** A simulated person: ids are per role (`dev-001`; added people `dev-added-001`). */
export interface Person {
  id: string;
  name: string;
  role: string;
  login: string;
  added?: boolean;
  [field: string]: unknown;
}

export interface Problem {
  path: string;
  message: string;
}

/** `POST /api/templates/{id}/preview`: the world the choices make, and what blocks building it. */
export interface Preview {
  name: string;
  title: string;
  domain: string;
  parts: { part: string; integration: string; system: string }[];
  hosts: string[];
  people: Person[];
  events: { id: string; label: string; kind: 'trap' | 'event'; on: boolean }[];
  tasks: { id: string; reason: string; checks: number }[];
  dropped: { what: string; why: string }[];
  needs: { images: { image: string; present: boolean; hint?: string }[]; memory_mb: number };
  problems: Problem[];
}

/** A benchmark a user can choose (`GET /api/worlds`), and with `elapsed_ms` and `error` one world's state. */
export interface BuiltWorld {
  name: string;
  title: string;
  template: { id: string; name: string } | null;
  built: boolean;
  current: boolean;
  state: 'preparing' | 'ready' | 'failed' | 'not_prepared';
  elapsed_ms?: number;
  error?: string | null;
}
