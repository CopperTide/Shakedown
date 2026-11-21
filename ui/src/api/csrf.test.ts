import { afterEach, describe, expect, it } from 'vitest';
import { CSRF_HEADER, remember, resetCsrf, withCsrf } from './csrf';

afterEach(() => resetCsrf());

const answered = (token?: string) => new Response('{}', { headers: token ? { [CSRF_HEADER]: token } : {} });

describe('the write token a deployment hands out', () => {
  it('is sent on writes once an answer carried one, and never on reads', () => {
    expect(withCsrf({ method: 'POST' })).toEqual({ method: 'POST' });
    remember(answered('t1'));
    expect(new Headers(withCsrf({ method: 'POST', headers: { 'content-type': 'application/json' } })?.headers).get(CSRF_HEADER)).toBe('t1');
    expect(new Headers(withCsrf({ method: 'delete' })?.headers).get(CSRF_HEADER)).toBe('t1');
    expect(withCsrf(undefined)).toBeUndefined();
    expect(withCsrf({ method: 'GET' })).toEqual({ method: 'GET' });
  });
  it('keeps the latest token, and an answer without one leaves it', () => {
    remember(answered('t1'));
    remember(answered());
    remember(answered('t2'));
    expect(new Headers(withCsrf({ method: 'PUT' })?.headers).get(CSRF_HEADER)).toBe('t2');
  });
  it('keeps the content type it was given', () => {
    remember(answered('t1'));
    expect(new Headers(withCsrf({ method: 'POST', headers: { 'content-type': 'application/json' } })?.headers).get('content-type')).toBe('application/json');
  });
});
