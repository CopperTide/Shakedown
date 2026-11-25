// Which Shakedown core (shaked) the UI talks to. Same origin by default; a build can name one with
// VITE_SHAKEDOWN_URL, and the address can choose another with `?core=<url>`, which this browser then
// remembers (`?core=` with nothing clears it). A core on another origin must allow this UI's
// origin: start it with SHAKEDOWN_CORS_ORIGIN=<the UI's origin>.

export const CORE_KEY = 'shakedown.core';
/** The core this build was made for: `VITE_SHAKEDOWN_URL`. */
const BUILT_CORE: string | undefined = import.meta.env.VITE_SHAKEDOWN_URL;

/** A core URL as a base for paths: http(s) only, no trailing slash; '' (same origin) for blank, null for anything else. */
export function normalizeCore(raw: string | null | undefined): string | null {
  const t = (raw ?? '').trim();
  if (!t) return '';
  try {
    const u = new URL(t);
    if (u.protocol !== 'http:' && u.protocol !== 'https:') return null;
    return `${u.origin}${u.pathname}`.replace(/\/+$/, '');
  } catch {
    return null;
  }
}

/** The path a build is served under (Vite's `BASE_URL`) without its trailing slash: '' for `/`, `/sd` for `/sd/`. */
export const basePath = (base: string | undefined) => (base ?? '/').replace(/\/+$/, '');

/**
 * The core to use and what to remember: `?core=` wins (and is remembered), then the remembered one, then the build's,
 * else the path this page is served under (`own`: a UI built for `/sd/` talks to the core behind `/sd`).
 */
export function resolveCore(search: string, stored: string | null, built: string | undefined, own = ''): { base: string; remember: string | null } {
  const q = new URLSearchParams(search);
  if (q.has('core')) {
    const asked = normalizeCore(q.get('core'));
    if (asked != null) return { base: asked, remember: asked };
  }
  return { base: normalizeCore(stored) || normalizeCore(built) || own, remember: null };
}

/** `path` (`/api/…`, `/mcp/…`) on the core at `base`. */
export const coreUrl = (path: string, base: string) => `${base}${path}`;

function storage(): Storage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

function initial(): string {
  if (typeof window === 'undefined') return normalizeCore(BUILT_CORE) || basePath(import.meta.env.BASE_URL);
  const s = storage();
  let stored: string | null = null;
  try { stored = s?.getItem(CORE_KEY) ?? null; } catch { /* storage blocked */ }
  const { base, remember } = resolveCore(window.location.search, stored, BUILT_CORE, basePath(import.meta.env.BASE_URL));
  try {
    if (remember === '') s?.removeItem(CORE_KEY);
    else if (remember) s?.setItem(CORE_KEY, remember);
  } catch { /* storage blocked: it holds for this page only */ }
  return base;
}

/** The core this page talks to ('' for its own origin). */
export const CORE = initial();

/** `path` on this page's core. */
export const api = (path: string) => coreUrl(path, CORE);
