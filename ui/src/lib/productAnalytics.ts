// Product analytics in PostHog, for hosted Shakedown only: the UI served by CopperTide's gateway under
// /sd/. It starts when `/api/me` says `mode: hosted` and the gateway's `/gw/api/me` hands over a
// signed-in person and a PostHog key. A local or open-source shaked never says `hosted`, so posthog-js
// is never loaded there and nothing is sent. Options, masking and address templating are in
// `posthogShared.ts`, shared with CopperTide's web app; this file names Shakedown's routes and actions.
import { basePath } from '@/api/core';
import type { Me } from '@/api/me';
import type { ItemKind } from '@/model/project';
import { productAnalytics, routeTemplater, type Org, type Person, type PostHogSettings } from './posthogShared';

/** The core flow's steps, by name only: never what they are called or what they hold. */
export type ProductEvent =
  | 'project created'
  | `${'world' | 'scenario' | 'setup' | 'experiment'} created`
  | 'experiment run started'
  | 'experiment deployed'
  | 'run started'
  | 'fork made'
  | 'compare opened';

/** What making a project item counts as. */
export const CREATED: Record<ItemKind, ProductEvent> = {
  worlds: 'world created', scenarios: 'scenario created', setups: 'setup created', experiments: 'experiment created',
};

/** The UI's routes (App.tsx), with `{param}` where a name or id goes. */
const PAGES = [
  '/', '/experiments', '/experiments/new', '/experiments/new/direct', '/experiments/compare', '/experiments/file/{id}', '/experiments/{id}',
  '/worlds', '/worlds/new', '/worlds/{id}', '/scenarios', '/scenarios/{id}', '/setups', '/setups/{id}', '/run', '/runs', '/runs/{run}/{branch}',
  '/team', '/connect', '/compare/{a}/{b}', '/control/{run}/{branch}', '/world/{run}/{branch}',
  '/settings', '/settings/project', '/settings/models', '/settings/tokens', '/settings/advanced',
];
const BASE = basePath(import.meta.env.BASE_URL);

/** `/sd/worlds/acme-plant` → `/sd/worlds/:id`: a world called "new" is still `:id` wherever an id goes. */
export const routeTemplate = routeTemplater([BASE || '/', ...PAGES.map((p) => BASE + p)], BASE ? [BASE.slice(1)] : []);

/** The only props a step may carry: which kind of comparison opened. Never free text. */
export type StepProps = { of: 'runs' | 'experiments' };

const analytics = productAnalytics<ProductEvent, StepProps>(routeTemplate);

type GatewayMe = { user?: Person; org?: Org | null; posthog?: PostHogSettings | null };
const fromGateway = (url: string) => fetch(url, { credentials: 'same-origin' });

/** Starts PostHog when this shaked is hosted: the person, their org and the settings come from the gateway. */
export async function startIfHosted(me: Me | null, get: (url: string) => Promise<Response> = fromGateway): Promise<void> {
  if (me?.mode !== 'hosted') return;
  const res = await get('/gw/api/me').catch(() => null);
  if (!res?.ok) return;
  const body = (await res.json().catch(() => null)) as GatewayMe | null;
  if (body?.user) await analytics.start(body.posthog, body.user, body.org ?? null);
}

/** On sign-out: forget who this browser was. */
export const stopProductAnalytics = () => analytics.stop();

/** A step of the core flow, while PostHog runs (never on a local shaked). */
export const track = (event: ProductEvent, props?: StepProps) => analytics.track(event, props);

/** For `.then`: counts `event` when the call succeeds, and passes its answer on. */
export const tracked = (event: ProductEvent) => <T>(answer: T): T => {
  track(event);
  return answer;
};
