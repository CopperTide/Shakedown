// The builder page's model: the draft of choices the steps edit, starting from the template's defaults, and what
// blocks each step. Pure: the page (BuildPage) fetches, previews and builds.
import type { Choices, Person, Preview, RoleForm, TemplateForm } from '@/model/world';

export const STEPS = ['template', 'company', 'systems', 'people', 'events', 'review'] as const;
export type Step = typeof STEPS[number];

export const STEP_LABEL: Record<Step, string> = {
  template: 'Template', company: 'Company', systems: 'Systems', people: 'People', events: 'Events', review: 'Review',
};

export interface Draft {
  template: string | null;
  company: string;
  size: number;
  seed: number;
  systems: string[];
  options: Record<string, Record<string, string>>;
  counts: Record<string, number>;
  edits: Record<string, { name: string }>;
  added: { name: string; role: string }[];
  removed: string[];
  events: Record<string, { on: boolean; params: Record<string, number | string> }>;
}

/** The template's defaults: its company, size, and the systems that are required or on by default. */
export function draftFor(form: TemplateForm): Draft {
  return {
    template: form.id, company: form.company.name, size: form.size.default, seed: 1,
    systems: form.systems.filter((s) => s.required || s.default).map((s) => s.id),
    options: {}, counts: {}, edits: {}, added: [], removed: [], events: {},
  };
}

/** `ids` and every system they require, transitively. */
function withRequired(form: TemplateForm, ids: string[]): Set<string> {
  const out = new Set(ids);
  for (let grew = true; grew;) {
    grew = false;
    for (const s of form.systems) {
      if (!out.has(s.id)) continue;
      for (const r of s.requires) if (!out.has(r)) { out.add(r); grew = true; }
    }
  }
  return out;
}

/** `ids` without `gone` and every system that requires one of them, transitively (required ones stay). */
function withoutDependents(form: TemplateForm, ids: string[], gone: string): Set<string> {
  const out = new Set(ids.filter((i) => i !== gone));
  for (let shrank = true; shrank;) {
    shrank = false;
    for (const s of form.systems) {
      if (out.has(s.id) && !s.required && s.requires.some((r) => !out.has(r))) { out.delete(s.id); shrank = true; }
    }
  }
  return out;
}

/** Turns a system on (with what it requires) or off (with what requires it); a required one stays on. */
export function toggleSystem(d: Draft, form: TemplateForm, id: string, on: boolean): Draft {
  const system = form.systems.find((s) => s.id === id);
  if (!system || (!on && system.required)) return d;
  const chosen = on ? withRequired(form, [...d.systems, id]) : withoutDependents(form, d.systems, id);
  return { ...d, systems: form.systems.filter((s) => chosen.has(s.id)).map((s) => s.id) };
}

/** A role's count: set by hand, else its share of the size rounded half up (never under its minimum), or fixed. */
export function roleCount(form: TemplateForm, d: Draft, role: string): number {
  if (d.counts[role] !== undefined) return d.counts[role];
  const r = form.roles.find((x) => x.id === role);
  if (!r) return 0;
  const n = 'fixed' in r.count ? r.count.fixed : Math.floor(r.count.share * d.size + 0.5);
  return Math.max(n, r.min);
}

/** The roles kept with the chosen systems. */
export function keptRoles(form: TemplateForm, d: Draft): RoleForm[] {
  return form.roles.filter((r) => r.systems.some((s) => d.systems.includes(s)));
}

/** The draft without edits and removals of people `people` no longer has. */
export function prune(d: Draft, people: Person[]): Draft {
  const ids = new Set(people.map((p) => p.id));
  const edits = Object.fromEntries(Object.entries(d.edits).filter(([id]) => ids.has(id)));
  return { ...d, edits, removed: d.removed.filter((id) => ids.has(id)) };
}

function companyProblem(d: Draft, form: TemplateForm): string | null {
  if (!d.company.trim()) return 'Give the company a name';
  const { min, max } = form.size;
  return d.size < min || d.size > max || !Number.isInteger(d.size) ? `Size is between ${min} and ${max}` : null;
}

function peopleProblem(d: Draft, form: TemplateForm): string | null {
  const short = keptRoles(form, d).find((r) => roleCount(form, d, r.id) < r.min);
  return short ? `${short.label} need at least ${short.min}` : null;
}

/** What blocks leaving `step`, in words; null when nothing does. Review waits for the preview, then its first problem. */
export function stepProblem(d: Draft, form: TemplateForm | null, step: Step, preview: Preview | null): string | null {
  if (step === 'template' || !form) return d.template && form ? null : 'Choose a template';
  if (step === 'company') return companyProblem(d, form);
  if (step === 'people') return peopleProblem(d, form);
  if (step === 'review') return preview ? (preview.problems[0]?.message ?? null) : 'Checking the world…';
  return null;
}

/** The choices the builder gets for the draft. */
export function choicesOf(d: Draft): Choices {
  return {
    seed: d.seed, company: { name: d.company.trim() }, size: d.size, systems: d.systems, options: d.options,
    people: { counts: d.counts, edits: d.edits, added: d.added, removed: d.removed }, events: d.events,
  };
}

/** "About 2.8 GB of memory" for the chosen systems; in MB under 1 GB. */
export function memoryLine(form: TemplateForm, d: Draft): string {
  const mb = form.systems.filter((s) => d.systems.includes(s.id)).reduce((sum, s) => sum + s.memory_mb, 0);
  return mb >= 1000 ? `About ${(mb / 1000).toFixed(1)} GB of memory` : `About ${mb} MB of memory`;
}
