// Dev only: a small generic world with views, for the fixture API (dev/fixtureApi.ts) to serve when no
// real payloads are dropped into public/dev-fixtures/world-views/. Sites on a ring, units moving jobs
// between them over one day; everything here is made up and names nothing real.
import type { ScreenData, Tone, ViewDef, WorldViews } from '@/features/views/types';
import type { BranchDetail, Event } from '@/model/types';

const H = 3_600_000, MIN = 60_000;
export const EPOCH = Date.parse('2026-11-22T00:00:00Z');
const DAY_START = 5 * H;

/** A small deterministic PRNG, so every reload draws the same world. */
function rng(seed: number) {
  let s = seed >>> 0;
  return () => { s = (s * 1664525 + 1013904223) >>> 0; return s / 2 ** 32; };
}

const pad = (n: number, w = 2) => String(n).padStart(w, '0');

interface Site { key: string; lat: number; lon: number }
interface Job { key: string; unit: number; o: number; d: number; depart: number; arrive: number; delay: number; held: boolean; load: number }

const SITES: Site[] = Array.from({ length: 14 }, (_, i) => {
  const a = (i / 14) * Math.PI * 2, r = 6 + (i % 3) * 2.5;
  return { key: `S${pad(i + 1)}`, lat: 48 + Math.sin(a) * r * 0.6, lon: 8 + Math.cos(a) * r };
});

const UNITS = 30;

const JOBS: Job[] = (() => {
  const rand = rng(7);
  const out: Job[] = [];
  for (let u = 0; u < UNITS; u++) {
    let t = DAY_START + rand() * 2 * H, at = Math.floor(rand() * SITES.length);
    for (let k = 0; k < 3; k++) {
      const to = (at + 1 + Math.floor(rand() * (SITES.length - 1))) % SITES.length;
      const block = (1.2 + rand() * 2.6) * H;
      const delay = rand() < 0.25 ? Math.round(10 + rand() * 90) : 0;
      out.push({ key: `J${pad(out.length + 1, 3)}`, unit: u, o: at, d: to, depart: t + delay * MIN, arrive: t + delay * MIN + block, delay, held: rand() < 0.05, load: 0.4 + rand() * 0.6 });
      t += block + delay * MIN + (0.6 + rand() * 1.6) * H;
      at = to;
    }
  }
  return out;
})();

type JobState = 'scheduled' | 'moving' | 'done' | 'held';

function stateOf(j: Job, clock: number): JobState {
  if (j.held && clock >= j.depart - j.delay * MIN - H) return 'held';
  if (clock < j.depart) return 'scheduled';
  return clock < j.arrive ? 'moving' : 'done';
}

function toneOf(j: Job, clock: number): [Tone, string] {
  const s = stateOf(j, clock);
  if (s === 'held') return ['critical', 'Held'];
  if (j.delay >= 60 && s !== 'done') return ['critical', `+${j.delay}`];
  if (j.delay >= 15 && s !== 'done') return ['warn', `+${j.delay}`];
  if (s === 'moving') return ['active', 'Moving'];
  return s === 'done' ? ['done', 'Done'] : ['neutral', 'Planned'];
}

export const VIEW: ViewDef = {
  id: 'ops', label: 'Network operations', part: 'sim', styles: {},
  panels: {
    kpis: { kind: 'kpis', items: [
      { label: 'On time', format: 'percent' }, { label: 'Moving', format: 'int' }, { label: 'Late', format: 'int' },
      { label: 'Held', format: 'int' }, { label: 'Mean delay', format: 'minutes' }, { label: 'Cost today', format: 'usd' }, { label: 'Network', format: 'chip' },
    ] },
    network: { kind: 'map', title: 'Network', center: [8, 48], zoom: 4.6, layers: [
      { id: 'sites', kind: 'points' }, { id: 'routes', kind: 'arcs' }, { id: 'units', kind: 'moving' },
    ] },
    jobs: { kind: 'board', title: 'Jobs', columns: [
      { field: 'id', label: 'Job' }, { field: 'unit', label: 'Unit' }, { field: 'origin', label: 'From' }, { field: 'dest', label: 'To' },
      { field: 'depart', label: 'Departs', format: 'time' }, { field: 'arrive', label: 'Arrives', format: 'time' },
      { field: 'delay_min', label: 'Delay', format: 'minutes' }, { field: 'load', label: 'Load', format: 'ratio_pct' }, { field: 'status', label: 'Status', format: 'chip' },
    ], sort: { field: 'depart' }, filters: [{ id: 'late', label: 'Late' }, { id: 'held', label: 'Held' }, { id: 'moving', label: 'Moving' }] },
    units: { kind: 'timeline', title: 'Units' },
    limits: { kind: 'bars', title: 'Shift limits', unit: 'h', decimals: 1 },
    log: { kind: 'feed', title: 'Events' },
  },
  screens: [
    { id: 'overview', label: 'Overview', areas: ['kpis kpis', 'network log', 'network limits'], columns: 'minmax(0, 1fr) 380px', rows: 'auto minmax(0, 1.3fr) minmax(0, 1fr)' },
    { id: 'jobs', label: 'Jobs', areas: ['kpis kpis', 'jobs units'], columns: 'minmax(0, 1fr) minmax(0, 1fr)', rows: 'auto minmax(0, 1fr)' },
    { id: 'units', label: 'Units', areas: ['kpis', 'units'], rows: 'auto minmax(0, 1fr)' },
  ],
};

export const DEFS: WorldViews = { world: 'synthetic-network', views: [VIEW] };

const siteRow = (s: Site, clock: number): unknown[] => {
  const late = JOBS.filter((j) => (j.o === SITES.indexOf(s)) && toneOf(j, clock)[0] === 'critical').length;
  const tone: Tone = late >= 2 ? 'critical' : late === 1 ? 'warn' : 'neutral';
  return [s.key, s.key, s.lat, s.lon, tone, tone === 'neutral' ? null : `${late} late`];
};

function mapData(clock: number) {
  const w = EPOCH + clock;
  const path = (j: Job) => { const [tone, chip] = toneOf(j, clock); const o = SITES[j.o], d = SITES[j.d]; return [j.key, j.key, o.lat, o.lon, d.lat, d.lon, EPOCH + j.depart, EPOCH + j.arrive, tone, tone === 'active' ? null : chip]; };
  const cols = ['key', 'label', 'olat', 'olon', 'dlat', 'dlon', 'depart', 'arrive', 'tone', 'chip'];
  return { kind: 'map' as const, layers: {
    sites: { columns: ['key', 'label', 'lat', 'lon', 'tone', 'chip'], rows: SITES.map((s) => siteRow(s, clock)) },
    routes: { columns: cols, rows: JOBS.filter((j) => stateOf(j, clock) === 'scheduled' && j.depart - clock < 2 * H).map(path) },
    units: { columns: cols, rows: JOBS.filter((j) => EPOCH + j.depart <= w && w < EPOCH + j.arrive && !j.held).map(path) },
  } };
}

function boardData(clock: number) {
  const rows = JOBS.map((j) => {
    const [tone, chip] = toneOf(j, clock);
    const s = stateOf(j, clock);
    const bits = (j.delay >= 15 ? 1 : 0) | (s === 'held' ? 2 : 0) | (s === 'moving' ? 4 : 0);
    return [j.key, j.key, `U${pad(j.unit + 1)}`, SITES[j.o].key, SITES[j.d].key, EPOCH + j.depart, EPOCH + j.arrive, j.delay, Math.round(j.load * 100) / 100, s, tone, chip, bits];
  });
  return { kind: 'board' as const, columns: ['key', 'id', 'unit', 'origin', 'dest', 'depart', 'arrive', 'delay_min', 'load', 'status', 'tone', 'chip', 'filters'], rows: rows.slice(0, 60), total: rows.length };
}

function timelineData(clock: number) {
  const lanes = Array.from({ length: UNITS }, (_, u) => `U${pad(u + 1)}`);
  const rows = JOBS.map((j) => { const [tone, chip] = toneOf(j, clock); return [j.unit, j.key, `${SITES[j.o].key}–${SITES[j.d].key}`, EPOCH + j.depart, EPOCH + j.arrive, tone, chip]; });
  return { kind: 'timeline' as const, lanes, columns: ['lane', 'key', 'label', 'start', 'end', 'tone', 'chip'], rows, range: [EPOCH + 4 * H, EPOCH + 24 * H] as [number, number] };
}

function barsData(clock: number) {
  const rows = Array.from({ length: 10 }, (_, i) => {
    const limit = 10 + (i % 3);
    const value = Math.max(0, ((clock - DAY_START) / H) * (0.55 + (i * 37 % 10) / 14));
    const ratio = value / limit;
    const tone: Tone = ratio >= 1 ? 'critical' : ratio >= 0.85 ? 'warn' : 'neutral';
    return [`R${pad(i + 1)}`, `R${pad(i + 1)}`, Math.round(value * 10) / 10, limit, ratio, tone, ratio >= 1 ? 'Over' : null];
  }).sort((a, b) => Number(b[4]) - Number(a[4]));
  return { kind: 'bars' as const, columns: ['key', 'label', 'value', 'limit', 'ratio', 'tone', 'chip'], rows };
}

function feedData(clock: number) {
  const evs: [number, string, string, Tone][] = [];
  for (const j of JOBS) {
    if (j.delay >= 15 && j.depart - j.delay * MIN <= clock) evs.push([j.depart - j.delay * MIN, 'delay', `${j.key} at ${SITES[j.o].key} delayed ${j.delay} min`, j.delay >= 60 ? 'critical' : 'warn']);
    if (j.held && j.depart - j.delay * MIN - H <= clock) evs.push([j.depart - j.delay * MIN - H, 'hold', `${j.key} held at ${SITES[j.o].key}`, 'critical']);
    if (j.arrive <= clock && j.delay === 0 && j.unit % 4 === 0) evs.push([j.arrive, 'arrival', `${j.key} arrived at ${SITES[j.d].key}`, 'neutral']);
  }
  evs.sort((a, b) => b[0] - a[0]);
  return { kind: 'feed' as const, columns: ['seq', 'clock_ms', 'part', 'kind', 'text', 'tone', 'chip'], rows: evs.slice(0, 40).map((e, i) => [1000 - i, e[0], 'sim', e[1], e[2], e[3], null]) };
}

function kpisData(clock: number) {
  const started = JOBS.filter((j) => stateOf(j, clock) !== 'scheduled');
  const late = started.filter((j) => j.delay >= 15).length;
  const onTime = started.length ? (100 * (started.length - late)) / started.length : 100;
  const moving = JOBS.filter((j) => stateOf(j, clock) === 'moving').length;
  const held = JOBS.filter((j) => stateOf(j, clock) === 'held').length;
  const meanDelay = started.length ? started.reduce((s, j) => s + j.delay, 0) / started.length : 0;
  const state: [Tone, string] = held > 1 ? ['critical', 'Disrupted'] : late > 4 ? ['warn', 'Strained'] : ['done', 'Normal'];
  return { kind: 'kpis' as const, items: [
    { value: onTime, tone: (onTime < 80 ? 'warn' : 'neutral') as Tone }, { value: moving, tone: 'active' as Tone }, { value: late, tone: (late ? 'warn' : 'neutral') as Tone },
    { value: held, tone: (held ? 'critical' : 'neutral') as Tone }, { value: meanDelay, tone: 'neutral' as Tone }, { value: started.length * 1840.5, tone: 'neutral' as Tone },
    { value: state[1], tone: state[0], chip: state[1] },
  ] };
}

const PANELS = { kpis: kpisData, network: mapData, jobs: boardData, units: timelineData, limits: barsData, log: feedData };

/** One screen of the synthetic view at a world clock. */
export function syntheticScreen(screen: string, clock: number, at: ScreenData['at']): ScreenData {
  const def = VIEW.screens.find((s) => s.id === screen) ?? VIEW.screens[0];
  const names = [...new Set(def.areas.flatMap((r) => r.split(/\s+/)))] as (keyof typeof PANELS)[];
  const panels = Object.fromEntries(names.filter((n) => PANELS[n]).map((n) => [n, PANELS[n](clock)])) as ScreenData['panels'];
  return { view: VIEW.id, screen: def.id, at, panels };
}

export const RUN_WV = 'run-wv';
export const WV_RECORDED = 'wv-recorded';
export const WV_LIVE = 'wv-live';
const STEPS = 48;
/** The world clock at step `k` of the synthetic run: twenty minutes a step from 05:00. */
export const clockAt = (k: number) => DAY_START + k * 20 * MIN;

function events(branch: string, team: BranchDetail['team'], checked: boolean): Event[] {
  const at = (seq: number) => new Date(Date.parse('2026-11-22T09:00:00Z') + seq * 2000).toISOString();
  const out: Event[] = [{ branch_id: branch, seq: 1, at: at(1), agent: null, kind: 'run_start', payload: { task_id: 'network-day', benchmark: 'synthetic', runner: 'external', team } }];
  for (let k = 1; k <= STEPS; k++) {
    const seq = k + 1;
    out.push(k % 6 === 0
      ? { branch_id: branch, seq, at: at(seq), agent: 'world', kind: 'world_event', payload: { part: 'sim', kind: 'tick', text: 'The world clock advanced', data: null, clock_ms: clockAt(k) } }
      : { branch_id: branch, seq, at: at(seq), agent: team.agents[0]?.id ?? 'agent', kind: 'tool_call', payload: { name: 'assign', args: {}, result: {}, error: null, reads: null, writes: null, snapshot: null, tool_call_id: null, part: 'sim', clock_ms: clockAt(k) } });
  }
  if (checked) out.push({ branch_id: branch, seq: STEPS + 2, at: at(STEPS + 2), agent: null, kind: 'check', payload: { passed: true, gold_db: null, differing_rows: [] } });
  return out;
}

/** The two fixture branches with views: one recorded (scrub it), one running (it polls live). */
export function viewBranches(team: BranchDetail['team']): Record<string, BranchDetail> {
  const base = { run_id: RUN_WV, parent_id: null, start_seq: null, team, changes: null, fork_note: null, error: null, created_at: '2026-11-22T09:00:00Z' };
  return {
    [WV_RECORDED]: { ...base, id: WV_RECORDED, status: 'passed', ended_at: '2026-11-22T09:02:00Z', events: events(WV_RECORDED, team, true) },
    [WV_LIVE]: { ...base, id: WV_LIVE, status: 'running', ended_at: null, events: events(WV_LIVE, team, false) },
  };
}

const clockOf = (e: Event | undefined) => Number((e?.payload as { clock_ms?: number } | undefined)?.clock_ms ?? clockAt(STEPS));

/** The synthetic answer for `at` on a fixture branch: a step's clock, the last clock for `final`, the wall clock live. */
export function syntheticAt(at: string | null, branch: BranchDetail): { clock: number; at: ScreenData['at'] } {
  if (at === 'live' || (at == null && branch.status === 'running')) {
    const clock = DAY_START + ((Date.now() * 60) % (17 * H));
    return { clock, at: { seq: null, point: 'live', clock_ms: clock, world_ms: EPOCH + clock, speed: 60, paused: false } };
  }
  const final = at === 'final' || at == null;
  const steps = branch.events.filter((e) => typeof (e.payload as { clock_ms?: unknown }).clock_ms === 'number');
  const e = final ? steps[steps.length - 1] : branch.events.find((x) => x.seq === Number(at));
  const clock = clockOf(e);
  return { clock, at: { seq: e?.seq ?? null, point: final ? 'final' : 'step', clock_ms: clock, world_ms: EPOCH + clock, speed: 0, paused: false } };
}
