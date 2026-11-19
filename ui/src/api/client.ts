// The one way the UI talks to arenad: typed requests, on paths built with `path` and `qs` (lib/url).
import { api } from './core';

export { path, qs } from '@/lib/url';

/** A request arenad refused: its status and arenad's `error` (else the body, else the status text). */
export class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message);
  }
}

/** `path` (`/api/…`) on this page's core; the JSON answer, or null for an empty one. */
export async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(api(path), init);
  const text = await res.text();
  let body: unknown = null;
  try {
    body = text ? JSON.parse(text) : null;
  } catch {
    /* non-JSON body */
  }
  if (!res.ok) {
    const msg = (body as { error?: string } | null)?.error ?? (text || res.statusText);
    throw new ApiError(res.status, msg);
  }
  return body as T;
}

export const get = <T>(path: string) => request<T>(path);

/** POST with a JSON body, or a bare POST when there is none. */
export const post = <T>(path: string, body?: unknown) =>
  request<T>(path, body === undefined
    ? { method: 'POST' }
    : { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) });

export const del = <T>(path: string) => request<T>(path, { method: 'DELETE' });
