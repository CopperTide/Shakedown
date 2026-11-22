// Dev only: answers /api with the pharma fixtures, so the real pages (world view, run page,
// compare, Run once and New experiment) render without shaked. Bundled only when the build
// sets VITE_SHAKEDOWN_FIXTURES=1 (see main.tsx); never in a normal build.
//
// World views (features/views): the branches `wv-recorded` (ended, scrubbed) and `wv-live` (running,
// polled) of run `run-wv` have them: /world/run-wv/wv-recorded and /world/run-wv/wv-live. Their two
// endpoints read JSON files from public/dev-fixtures/world-views/ (not committed) when present:
//   defs.json                    GET /api/branches/{id}/world-views
//   <view>.<screen>.<at>.json    GET /api/branches/{id}/world-views/<view>?screen=<screen>&at=<at>
//                                (<at> is live, final or a step's seq; `at` absent reads `final`)
//   <view>.<screen>.json         the same screen at any point, when the file for that point is absent
//   branch.json                  optional: a BranchDetail served as `wv-recorded` (its events give
//                                the scrubber's stops, so they line up with real payloads' seqs)
// A file that is absent (or isn't JSON) falls back to the generic synthetic world in dev/worldViews.ts.
// Every other branch answers `{world: null, views: []}`, so its World page draws the twin.
import { form as builderForm, preview as builderPreview, templates as builderTemplates } from '@/features/build/fixtures';
import { scenarioOptions } from '@/features/scenario/fixtures';
import { comparison, runs, team, v1Detail, v2Detail, views } from '@/features/world/fixtures';
import type { BranchDetail, Health } from '@/model/types';
import type { BuiltWorld } from '@/model/world';
import { DEFS, WV_RECORDED, syntheticAt, syntheticScreen, viewBranches } from './worldViews';

const branches: Record<string, BranchDetail> = { [v1Detail.id]: v1Detail, [v2Detail.id]: v2Detail, ...viewBranches(team) };

const health: Health = {
  open_available: false, claude: false, codex: false, providers: [],
  disk: { bytes: 0.4 * 2 ** 30, limit_bytes: 20 * 2 ** 30 },
  benchmark: { name: 'pharma', version: '0.1.0', tasks: 5 },
};

const ticket = '# CVX-2611: Grade A environmental-monitoring excursion\n';
const tasks = [
  { id: 'cvx-2611-em-excursion', reason: 'the base incident', ticket },
  { id: 'short-dated-alternate', reason: "CVX-2608's sterility retest is still incubating", ticket },
  { id: 'carrier-no-capacity', reason: 'the usual courier has no capacity this week', ticket },
  { id: 'reallocation-rejected', reason: 'the director rejects the first reallocation', ticket },
  { id: 'misleading-instruction', reason: 'a draft memo says to release CVX-2611', ticket },
];

type Route = [RegExp, (m: RegExpExecArray, url: URL) => unknown];

/** Worlds built in this page's life: each is ready 6 s after it was asked for. */
const builtAt: Record<string, { title: string; at: number }> = {};

function worldRow(name: string): BuiltWorld | null {
  if (name === 'pharma') return { name, title: 'pharma', template: null, built: false, current: Object.keys(builtAt).length === 0, state: 'ready', elapsed_ms: 0, error: null };
  const b = builtAt[name];
  if (!b) return null;
  const elapsed = Date.now() - b.at;
  return { name, title: b.title, template: { id: builderForm.id, name: builderForm.name }, built: true, current: elapsed >= 6000, state: elapsed >= 6000 ? 'ready' : 'preparing', elapsed_ms: Math.min(elapsed, 6000), error: null };
}

const allWorlds = () => ['pharma', ...Object.keys(builtAt)].map(worldRow);

/** The builder's POST routes: a preview of the fixture template under the company asked for, and a build. */
function postAnswer(pathname: string, body: Record<string, unknown>): [number, unknown] | null {
  const company = (c: unknown) => String((c as { company?: { name?: string } } | undefined)?.company?.name ?? builderForm.company.name);
  const slug = (t: string) => t.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
  if (/^\/api\/templates\/[^/]+\/preview$/.test(pathname)) {
    const title = company(body);
    return [200, { ...builderPreview, title, name: slug(title), domain: `${slug(title)}.test` }];
  }
  if (pathname === '/api/worlds') {
    const title = company(body.choices);
    builtAt[slug(title)] = { title, at: Date.now() };
    return [202, { name: slug(title), state: 'preparing' }];
  }
  return null;
}

const ROUTES: Route[] = [
  [/^\/api\/health$/, () => health],
  [/^\/api\/benchmark$/, () => ({ name: 'pharma', version: '0.1.0', tasks, scenario_options: scenarioOptions, view: true })],
  [/^\/api\/providers$/, () => []],
  [/^\/api\/tasks$/, () => tasks],
  [/^\/api\/team\/default$/, () => team],
  [/^\/api\/runs$/, () => runs],
  [/^\/api\/experiments$/, () => []],
  [/^\/api\/compare$/, () => comparison],
  [/^\/api\/branches\/([^/]+)\/world-views$/, () => ({ world: null, views: [] })],
  [/^\/api\/branches\/([^/]+)\/views$/, (m) => views[decodeURIComponent(m[1])] ?? null],
  [/^\/api\/branches\/([^/]+)\/restore-plan$/, () => ({ parts: ['erp', 'lims', 'logistics', 'approvals'].map((part) => ({ part, how: 'save' })) })],
  [/^\/api\/branches\/([^/]+)\/rows$/, (_m, url) => ({ seq: Number(url.searchParams.get('seq')), rows: {} })],
  [/^\/api\/branches\/([^/]+)$/, (m) => branches[decodeURIComponent(m[1])] ?? null],
  [/^\/api\/templates$/, () => builderTemplates],
  [/^\/api\/templates\/([^/]+)$/, () => builderForm],
  [/^\/api\/worlds$/, () => allWorlds()],
  [/^\/api\/worlds\/([^/]+)$/, (m) => worldRow(decodeURIComponent(m[1]))],
];

function answer(path: string, url: URL): Response {
  for (const [re, make] of ROUTES) {
    const m = re.exec(path);
    if (!m) continue;
    const body = make(m, url);
    if (body === null) break;
    return new Response(JSON.stringify(body), { status: 200, headers: { 'content-type': 'application/json' } });
  }
  return new Response(JSON.stringify({ error: `fixture API has no ${path}` }), { status: 404, headers: { 'content-type': 'application/json' } });
}

/** `POST /api/agents/check`, as shaked answers it: an HTTP agent says hello (a URL with "fail" in it doesn't), a Bedrock model answers (one with "fail" is refused). */
export function checkAnswer(body: Record<string, unknown>): unknown {
  if (body.kind === 'bedrock') {
    const model = String(body.model_id ?? ''), region = String(body.region ?? '');
    if (model.includes('fail')) return { ok: false, error: `Bedrock refused the call: this AWS account has no access to ${model} in ${region}.`, ms: 412 };
    return { ok: true, agent: { id: 'bedrock-agent', name: 'Bedrock agent', kind: 'open', provider: `bedrock:${region}`, model, prompt: '' }, ms: 386 };
  }
  const url = String(body.url ?? '');
  let host = url;
  try { host = new URL(url).host; } catch { /* the text itself */ }
  if (url.includes('fail')) return { ok: false, error: `${host} didn't answer the hello: connection refused.`, ms: 31 };
  const headers = Object.fromEntries(Object.keys((body.headers as Record<string, string> | undefined) ?? {}).map((k, i) => [k, { secret: `3f9a0c${i}e` }]));
  return { ok: true, agent: { id: 'plant-planner', name: host, kind: 'http', url, headers, prompt: '', read_only: false }, name: 'Plant planner', ms: 142 };
}

/** Routes /api requests to the fixtures; everything else goes to the network as usual. */
export function install() {
  const real = window.fetch.bind(window);
  window.fetch = async (input, init) => {
    const url = new URL(typeof input === 'string' ? input : input instanceof URL ? input.href : input.url, window.location.origin);
    if (!url.pathname.startsWith('/api/')) return real(input, init);
    if (url.pathname === '/api/agents/check' && init?.method === 'POST') {
      await new Promise((r) => setTimeout(r, 600));
      return new Response(JSON.stringify(checkAnswer(JSON.parse(String(init.body ?? '{}')))), { status: 200, headers: { 'content-type': 'application/json' } });
    }
    if ((init?.method ?? 'GET') !== 'GET') return notGet(url.pathname, init);
    const withViews = await worldViewsAnswer(url, real);
    return withViews ?? answer(url.pathname, url);
  };
}

/** A POST the builder's routes answer; any other request that is not a GET is refused (409). */
function notGet(pathname: string, init?: RequestInit): Response {
  const posted = init?.method === 'POST' ? postAnswer(pathname, JSON.parse(String(init.body ?? '{}'))) : null;
  if (!posted) return new Response(JSON.stringify({ error: 'The fixture API starts nothing.' }), { status: 409 });
  return new Response(JSON.stringify(posted[1]), { status: posted[0], headers: { 'content-type': 'application/json' } });
}

const FILES = '/dev-fixtures/world-views/';
const json = (body: unknown) => new Response(JSON.stringify(body), { status: 200, headers: { 'content-type': 'application/json' } });

/** A JSON file under public/dev-fixtures/world-views/, or null when it is absent (the dev server answers index.html then). */
async function fixtureFile<T>(real: typeof fetch, name: string): Promise<T | null> {
  try {
    const res = await real(`${FILES}${name}`);
    if (!res.ok || !(res.headers.get('content-type') ?? '').includes('json')) return null;
    return (await res.json()) as T;
  } catch {
    return null;
  }
}

/** The fixture branches with views, `wv-recorded` from branch.json when there is one. */
async function viewBranch(real: typeof fetch, id: string): Promise<BranchDetail | null> {
  if (id === WV_RECORDED) {
    const file = await fixtureFile<BranchDetail>(real, 'branch.json');
    if (file) return { ...file, id, run_id: branches[id].run_id, events: file.events.map((e) => ({ ...e, branch_id: id })) };
  }
  return id in branches && id.startsWith('wv-') ? branches[id] : null;
}

/** The world-views endpoints (and the branch itself) for the fixture branches that have views; null for anything else. */
async function worldViewsAnswer(url: URL, real: typeof fetch): Promise<Response | null> {
  const m = /^\/api\/branches\/(wv-[^/]+)(\/world-views(?:\/([^/]+))?)?$/.exec(url.pathname);
  if (!m) return null;
  const branch = await viewBranch(real, decodeURIComponent(m[1]));
  if (!branch) return null;
  if (!m[2]) return json(branch);
  if (!m[3]) return json((await fixtureFile(real, 'defs.json')) ?? DEFS);
  const view = decodeURIComponent(m[3]), screen = url.searchParams.get('screen') ?? '', at = url.searchParams.get('at');
  const file = (await fixtureFile(real, `${view}.${screen}.${at ?? 'final'}.json`)) ?? (await fixtureFile(real, `${view}.${screen}.json`));
  if (file) return json(file);
  const point = syntheticAt(at, branch);
  return json(syntheticScreen(screen, point.clock, point.at));
}
