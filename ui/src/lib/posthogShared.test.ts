// @vitest-environment jsdom
// Tests of posthogShared.ts, kept identical in CopperTide (web/src/hosted) and Shakedown (ui/src/lib).
import posthog, { type CaptureResult, type CapturedNetworkRequest } from 'posthog-js';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { browserOptedOut, EUROPEAN_ZONES, inEurope, posthogOptions, productAnalytics, routeTemplater, templateUrl } from './posthogShared';

const template = routeTemplater(['/', '/team/{team}', '/team/{team}/git', '/projects/{project}/runs/{run}'], ['team', 'projects']);
const HOST = 'https://us.i.posthog.com';

beforeEach(() => {
  vi.stubGlobal('fetch', vi.fn(async () => new Response('{}', { status: 200 })));
  window.history.pushState({}, '', '/');
});
afterEach(() => vi.unstubAllGlobals());

it('masks all text, inputs and element attributes, in clicks and in replay, and honours Do Not Track', () => {
  expect(posthogOptions(HOST, false, template)).toMatchObject({
    api_host: HOST, respect_dnt: true, persistence: 'localStorage+cookie', disable_session_recording: false,
    mask_all_text: true, mask_all_element_attributes: true,
    session_recording: { maskAllInputs: true, maskTextSelector: '*', maskAllElementAttributes: true, recordHeaders: false, recordBody: false },
  });
});

it('pins off in code everything the dashboard could turn on', () => {
  expect(posthogOptions(HOST, false, template)).toMatchObject({
    enable_recording_console_log: false, capture_heatmaps: false, capture_exceptions: false, capture_performance: false,
    logs: { captureConsoleLogs: false }, session_recording: { captureCanvas: { recordCanvas: false } },
    advanced_disable_flags: true, disable_surveys: true, disable_conversations: true, disable_product_tours: true,
  });
});

it('stores nothing and records no replay in the EU and EEA, outermost regions included', () => {
  expect(posthogOptions(HOST, true, template)).toMatchObject({ persistence: 'memory', disable_session_recording: true });
  for (const tz of ['Europe/Berlin', 'Asia/Nicosia', 'Atlantic/Reykjavik', 'Arctic/Longyearbyen', 'Indian/Reunion', 'America/Cayenne', 'Atlantic/Canary']) {
    expect(inEurope(tz), tz).toBe(true);
  }
  expect(inEurope('America/New_York')).toBe(false);
  expect(inEurope('Asia/Tokyo')).toBe(false);
  expect(EUROPEAN_ZONES).toContain('America/Martinique');
});

it('Global Privacy Control or Do Not Track keeps PostHog from starting at all', async () => {
  expect(browserOptedOut({ globalPrivacyControl: true })).toBe(true);
  expect(browserOptedOut({ doNotTrack: '1' })).toBe(true);
  expect(browserOptedOut({ doNotTrack: null })).toBe(false);
  const load = vi.fn(async () => posthog);
  Object.defineProperty(navigator, 'globalPrivacyControl', { configurable: true, value: true });
  await productAnalytics(template, load).start({ key: 'phc_test', host: HOST }, { id: 'u', email: 'e', name: 'n' }, null);
  expect(load).not.toHaveBeenCalled();
  Object.defineProperty(navigator, 'globalPrivacyControl', { configurable: true, value: undefined });
});

it('signing out stops the replay, forgets the person and opts out until the next start', async () => {
  const ph = { init: vi.fn(), opt_in_capturing: vi.fn(), identify: vi.fn(), group: vi.fn(), capture: vi.fn(), stopSessionRecording: vi.fn(), reset: vi.fn(), opt_out_capturing: vi.fn() };
  const a = productAnalytics<'x'>(template, async () => ph as never);
  await a.start({ key: 'phc_test', host: HOST }, { id: 'u', email: 'e', name: 'n' }, { id: 'o', name: 'Personal' });
  expect(ph.opt_in_capturing).toHaveBeenCalledWith({ captureEventName: null });
  a.stop();
  expect(ph.stopSessionRecording).toHaveBeenCalledOnce();
  expect(ph.reset.mock.invocationCallOrder[0]).toBeLessThan(ph.opt_out_capturing.mock.invocationCallOrder[0]);
  a.track('x');
  expect(ph.capture).not.toHaveBeenCalled();
});

it('addresses keep only fixed words, by position', () => {
  expect(template('/team/secret-plan/git')).toBe('/team/:id/git');
  expect(template('/team/git')).toBe('/team/:id'); // a team called "git" is still an id
  expect(template('/projects/acme-launch/runs/r1')).toBe('/projects/:id/runs/:id');
  expect(template('/elsewhere/secret')).toBe('/:id/:id');
  expect(templateUrl('https://app.example.com/team/secret-plan?q=secret#secret', template)).toBe('https://app.example.com/team/:id');
  expect(templateUrl('$direct', template)).toBe('$direct');
});

it("the replay's own addresses are templated", () => {
  const mask = posthogOptions(HOST, false, template).session_recording!.maskCapturedNetworkRequestFn!;
  const page = mask({ name: 'https://app.example.com/team/secret-plan/git?x=secret' } as CapturedNetworkRequest);
  expect(page?.name).toBe('https://app.example.com/team/:id/git');
});

it('no captured payload holds a team or project name, from posthog-js itself', () => {
  const sent: CaptureResult[] = [];
  const o = posthogOptions(HOST, false, template);
  const keep = (e: CaptureResult | null) => { if (e) sent.push(structuredClone(e)); return e; };
  window.history.pushState({}, '', '/team/secret-plan/git?q=secret-query#secret-hash');
  const ph = posthog.init('phc_test', { ...o, advanced_disable_flags: true, before_send: [o.before_send as never, keep] }, 'scrub-test')!;
  ph.capture('$pageview');
  window.history.pushState({}, '', '/projects/secret-project/runs/r9');
  ph.capture('$pageview');
  ph.capture('run downloaded');
  const all = JSON.stringify(sent);
  expect(sent.length).toBeGreaterThanOrEqual(3);
  expect(all).toContain('/team/:id/git');
  expect(all).not.toMatch(/secret/);
});
