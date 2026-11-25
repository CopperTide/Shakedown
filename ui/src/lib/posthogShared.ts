// Product analytics in PostHog for a hosted, signed-in session: the options, the address templating,
// and start, stop and track. This file is kept identical in CopperTide (web/src/hosted/posthogShared.ts)
// and Shakedown (ui/src/lib/posthogShared.ts): change both together. Each app passes its own routes.
// posthog-js is loaded only by `start`, so a build that never starts it never loads it.
import type { BeforeSendFn, CapturedNetworkRequest, PostHog, PostHogConfig, Properties } from 'posthog-js';

/** Where events go: PostHog's public project key and host, as the gateway hands them over. */
export type PostHogSettings = { key: string; host: string };
export type Person = { id: string; email: string; name: string };
export type Org = { id: string; name: string };
export type Template = (path: string) => string;

const fixedWords = (ps: string[]) => ps.filter((p) => !p.startsWith('{')).length;

/**
 * Templates paths by position against `patterns` (`/team/{team}/git` makes `/team/:id/git`); the
 * pattern with the most fixed words wins. A path matching none keeps only a leading word in `sections`.
 */
export function routeTemplater(patterns: string[], sections: string[]): Template {
  const split = patterns.map((p) => p.split('/'));
  return (pathname) => {
    const segs = pathname.split('/');
    const fits = split.filter((ps) => ps.length === segs.length && ps.every((p, i) => p.startsWith('{') || p === segs[i]));
    const best = fits.sort((a, b) => fixedWords(b) - fixedWords(a))[0];
    if (best) return best.map((p) => (p.startsWith('{') ? ':id' : p)).join('/');
    return segs.map((s, i) => (s === '' || (i === 1 && sections.includes(s)) ? s : ':id')).join('/');
  };
}

/** An address as its origin and templated path, with no query or fragment; other text as it is. */
export function templateUrl(v: string, template: Template): string {
  if (v.startsWith('/') && !v.startsWith('//')) return template(v.split(/[?#]/)[0]);
  try {
    const u = new URL(v);
    return u.origin + template(u.pathname);
  } catch {
    return v; // `$direct` and the like
  }
}

const ADDRESS_KEY = /url|referrer|href|pathname/i;

function scrub(props: Properties | undefined, template: Template) {
  if (!props) return;
  for (const [k, v] of Object.entries(props)) {
    if (typeof v === 'string' && ADDRESS_KEY.test(k)) props[k] = templateUrl(v, template);
  }
}

/** Every event leaves with each address property (`$current_url`, `$pathname`, `$referrer`…) templated. */
export const scrubber = (template: Template): BeforeSendFn => (e) => {
  [e.properties, e.$set, e.$set_once].forEach((p) => scrub(p, template));
  return e;
};

/**
 * Time zones in the EU and EEA, including the outermost regions (`Europe/` matches every European
 * zone, a little more than the EU). A list to edit, not logic.
 */
export const EUROPEAN_ZONES = [
  'Europe/', 'Asia/Nicosia', 'Asia/Famagusta', 'Atlantic/Reykjavik', 'Atlantic/Faroe', 'Arctic/Longyearbyen', 'Atlantic/Madeira',
  'Atlantic/Azores', 'Atlantic/Canary', 'Indian/Reunion', 'Indian/Mayotte', 'America/Guadeloupe', 'America/Martinique', 'America/Cayenne',
];

/** Europe, by the browser's time zone. There is no consent banner, so these visits store nothing and record no replay. */
export function inEurope(tz = Intl.DateTimeFormat().resolvedOptions().timeZone): boolean {
  return EUROPEAN_ZONES.some((z) => (z.endsWith('/') ? (tz ?? '').startsWith(z) : tz === z));
}

/** Whether the browser asks not to be tracked: Global Privacy Control or Do Not Track. Then nothing starts. */
export function browserOptedOut(nav: { globalPrivacyControl?: boolean; doNotTrack?: string | null } = navigator): boolean {
  return nav.globalPrivacyControl === true || nav.doNotTrack === '1';
}

/** The replay's own addresses (each page's URL, and any request) go through the same templates. */
const replayAddress = (template: Template) => (r: CapturedNetworkRequest): CapturedNetworkRequest =>
  (r.name ? { ...r, name: templateUrl(r.name, template) } : r);

/** Everything PostHog's dashboard could turn on that is not wanted here, pinned off in code. */
const OFF: Partial<PostHogConfig> = {
  enable_recording_console_log: false,
  capture_heatmaps: false,
  capture_exceptions: false,
  capture_performance: false, // no network timing: request addresses carry names
  logs: { captureConsoleLogs: false },
  advanced_disable_flags: true,
  disable_surveys: true,
  disable_conversations: true,
  disable_product_tours: true,
};

/** How PostHog runs: masked autocapture and replay, templated addresses, Do Not Track honoured. */
export function posthogOptions(host: string, europe: boolean, template: Template): Partial<PostHogConfig> {
  return {
    ...OFF,
    api_host: host,
    persistence: europe ? 'memory' : 'localStorage+cookie',
    disable_session_recording: europe,
    respect_dnt: true,
    person_profiles: 'identified_only',
    autocapture: true,
    capture_pageview: 'history_change',
    capture_pageleave: true,
    mask_all_text: true,
    mask_all_element_attributes: true,
    session_recording: {
      maskAllInputs: true, maskTextSelector: '*', maskAllElementAttributes: true, recordHeaders: false, recordBody: false,
      captureCanvas: { recordCanvas: false }, maskCapturedNetworkRequestFn: replayAddress(template),
    },
    before_send: scrubber(template),
  };
}

const loadPostHog = () => import('posthog-js').then((m) => m.default);

/** Starts `p` for `person` in `org`: the options, consent back on after a sign-out, identify, group. */
function launch(p: PostHog, s: PostHogSettings, person: Person, org: Org | null, template: Template) {
  p.init(s.key, posthogOptions(s.host, inEurope(), template));
  p.opt_in_capturing({ captureEventName: null }); // undoes the last sign-out's opt-out, sending nothing itself
  p.identify(person.id, { email: person.email, name: person.name });
  if (org) p.group('organization', org.id, { name: org.name });
}

/** On sign-out: no more replay, the person forgotten, and nothing captured until the next start. */
function halt(p: PostHog) {
  p.stopSessionRecording();
  p.reset(); // before the opt-out: reset() clears consent
  p.opt_out_capturing();
}

/**
 * One app's analytics: `start` once signed in with settings (never when the browser opts out), `stop`
 * on sign-out, `track` named actions with `P`, the only props they may carry.
 */
export function productAnalytics<E extends string, P extends Record<string, string> = never>(template: Template, load: () => Promise<PostHog> = loadPostHog) {
  let ph: PostHog | null = null;
  let starting: Promise<void> | null = null;
  let session = 0; // a sign-out while posthog-js loads cancels that start
  const begin = async (s: PostHogSettings, person: Person, org: Org | null) => {
    const mine = session;
    const p = await load();
    if (mine !== session) return;
    launch(p, s, person, org, template);
    ph = p;
  };
  return {
    start(settings: PostHogSettings | null | undefined, person: Person, org: Org | null): Promise<void> {
      if (!settings || browserOptedOut()) return Promise.resolve();
      starting ??= begin(settings, person, org).catch(() => { starting = null; });
      return starting;
    },
    stop() {
      if (ph) halt(ph);
      ph = null;
      starting = null;
      session += 1;
    },
    track(event: E, props?: P) {
      ph?.capture(event, props);
    },
  };
}
