// A deployment that checks writes for a security token (CopperTide's gateway, in front of hosted
// shaked) hands the token out in the `x-csrf-token` header of its answers. The UI keeps the latest
// one and sends it back on every write; a local shaked sends none, so nothing is added.

export const CSRF_HEADER = 'x-csrf-token';

let token: string | null = null;

/** Keeps the token `res` carries, if any. */
export function remember(res: { headers: Headers }): void {
  const t = res.headers.get(CSRF_HEADER);
  if (t) token = t;
}

/** `init` with the token added when it is a write and there is a token; else `init` unchanged. */
export function withCsrf(init?: RequestInit): RequestInit | undefined {
  const method = (init?.method ?? 'GET').toUpperCase();
  if (!token || method === 'GET' || method === 'HEAD') return init;
  const headers = new Headers(init?.headers);
  headers.set(CSRF_HEADER, token);
  return { ...init, headers };
}

/** Forgets the token (tests). */
export function resetCsrf(): void {
  token = null;
}
