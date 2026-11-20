// Neutral builder fixtures for tests and the dev fixture API: a template form as the builder answers `template`,
// its preview, and built worlds as shaked lists them.
import type { BuiltWorld, Person, Preview, TemplateForm, TemplateSummary } from '@/model/world';

export const form: TemplateForm = {
  id: 'example', name: 'Example', summary: 'A neutral template', version: '1.0.0',
  company: { name: 'Example Co' },
  size: { label: 'Staff', default: 10, min: 2, max: 40, presets: [5, 10, 20] },
  systems: [
    { id: 'code', label: 'Code host', product: 'Code twin (simulated)', required: true, default: true, requires: [], memory_mb: 900, hosts: ['code.{company.domain}'], options: [] },
    { id: 'tracker', label: 'Tracker', product: 'Tracker twin (simulated)', required: false, default: true, requires: [], memory_mb: 1200, hosts: [], options: [{ name: 'profile', label: 'Profile', choices: ['lean', 'full'], default: 'lean' }] },
    { id: 'chat', label: 'Chat', product: 'Chat twin (simulated)', summary: 'Where the team talks', required: false, default: false, requires: ['tracker'], memory_mb: 700, hosts: [], options: [] },
  ],
  roles: [
    { id: 'dev', label: 'Developers', summary: 'Write and review the code', count: { share: 0.75 }, min: 1, systems: ['code'] },
    { id: 'pm', label: 'Product managers', count: { fixed: 1 }, min: 1, systems: ['tracker'] },
  ],
  events: [
    { id: 'flaky', label: 'A test is flaky', description: 'One test fails {param.rate} in 10 runs', kind: 'trap', default: true, needs: ['code'], params: [{ name: 'rate', label: 'Fails in 10', kind: 'int', min: 1, max: 9, default: 2 }] },
    { id: 'outage', label: 'The tracker goes down', description: 'The tracker is down for an hour', kind: 'event', default: false, needs: ['tracker'], params: [] },
  ],
};

export const templates: TemplateSummary[] = [
  { id: 'example', name: 'Example', summary: 'A neutral template', version: '1.0.0', systems: ['Code host', 'Tracker', 'Chat'] },
  { id: 'other', name: 'Other example', summary: 'Another neutral template', version: '0.1.0', systems: ['Tracker'] },
];

const FIRST = ['Ada', 'Ben', 'Cy', 'Dee', 'Eve', 'Fay', 'Gil', 'Hal', 'Ivy', 'Jo', 'Kit', 'Lu'];

/** `n` developers (`dev-001`…) and one product manager. */
export function people(n: number): Person[] {
  const devs = Array.from({ length: n }, (_, i): Person => {
    const name = `${FIRST[i % FIRST.length]} Example`;
    return { id: `dev-${String(i + 1).padStart(3, '0')}`, name, role: 'dev', login: name.split(' ')[0].toLowerCase() + 'e' };
  });
  return [...devs, { id: 'pm-001', name: 'Pat Example', role: 'pm', login: 'pate' }];
}

export const preview: Preview = {
  name: 'example-co', title: 'Example Co', domain: 'example-co.test',
  parts: [{ part: 'code', integration: 'code-twin', system: 'code' }, { part: 'tracker', integration: 'tracker-twin', system: 'tracker' }],
  hosts: ['code.example-co.test'], people: people(8),
  events: [{ id: 'flaky', label: 'A test is flaky', kind: 'trap', on: true }, { id: 'outage', label: 'The tracker goes down', kind: 'event', on: false }],
  tasks: [{ id: 'fix-build', reason: 'The build is red', checks: 4 }],
  dropped: [{ what: 'event chat-spam', why: 'needs the system chat, not chosen' }],
  needs: { images: [{ image: 'code-twin:1', present: true, hint: 'make code-twin' }], memory_mb: 2100 },
  problems: [],
};

export const built: BuiltWorld = {
  name: 'example-co', title: 'Example Co', template: { id: 'example', name: 'Example' }, built: true, current: false, state: 'preparing', elapsed_ms: 4000, error: null,
};

export const worlds: BuiltWorld[] = [
  { name: 'retail', title: 'retail', template: null, built: false, current: true, state: 'ready' },
  { ...built, state: 'ready', elapsed_ms: undefined, error: undefined },
];
