import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { KeyRow } from '@/api/models';
import type { TokenRow } from '@/api/tokens';
import { checkLine, emptyKeyForm, keyBody, ModelsView } from './ModelsPage';
import { SCOPES, TokensView } from './TokensPage';

const token: TokenRow = { id: 't1', name: 'ci', prefix: 'sd_1a2b3', scopes: ['run'], created_at: '2026-11-20T10:00:00Z', last_used_at: null, expires_at: null };
const secret = `sd_${'f'.repeat(64)}`;
const drawTokens = (fresh: { id: string; token: string; prefix: string } | null) => renderToStaticMarkup(
  <TokensView tokens={[token]} fresh={fresh} form={{ name: '', scope: 'read' }} onChange={() => {}} onCreate={() => {}} onRevoke={() => {}} onDismiss={() => {}} />,
);

const key: KeyRow = { id: 'k1', name: 'pod', kind: 'custom', base_url: 'http://127.0.0.1:8000/v1', model: 'llama3', key_last4: '1234', checked_ok: true, checked_at: '2026-11-20T10:00:00Z', check_detail: '', created_at: '2026-11-20T10:00:00Z' };
const drawModels = (keys: KeyRow[], form = emptyKeyForm()) => renderToStaticMarkup(
  <ModelsView keys={keys} form={form} testing={null} error={null} onChange={() => {}} onAdd={() => {}} onTest={() => {}} onDelete={() => {}} />,
);

describe('API tokens', () => {
  it('shows a new token once, then only its prefix', () => {
    const once = drawTokens({ id: 't2', token: secret, prefix: 'sd_fffff' });
    expect(once).toContain(secret);
    expect(once).toContain('not shown again');
    const later = drawTokens(null);
    expect(later).not.toContain(secret);
    expect(later).toContain('sd_1a2b3…');
  });
  it('offers read, run and admin scopes and names what each allows', () => {
    expect(SCOPES.map((s) => s.id)).toEqual(['read', 'run', 'admin']);
    const html = drawTokens(null);
    for (const s of SCOPES) expect(html).toContain(s.allows);
    expect(SCOPES[2].allows).toMatch(/model keys/);
  });
});

describe('Models', () => {
  it('adds a custom server with a URL and runs Test', () => {
    const form = { ...emptyKeyForm(), kind: 'custom' as const, name: 'pod', base_url: ' http://127.0.0.1:8000/v1 ', model: 'llama3' };
    expect(keyBody(form)).toEqual({ kind: 'custom', name: 'pod', secret: '', base_url: 'http://127.0.0.1:8000/v1', model: 'llama3' });
    const html = drawModels([key], form);
    expect(html).toContain('placeholder="http://127.0.0.1:8000/v1"');
    expect(html).toContain('>Test<');
    expect(checkLine(key)).toBe('Works');
    expect(checkLine({ ...key, checked_ok: false, check_detail: 'the provider turned the key down (401)' })).toBe('⚠ the provider turned the key down (401)');
    expect(checkLine({ ...key, checked_ok: null })).toBe('Not tested');
  });
  it('never renders a key secret after saving', () => {
    const typed = { ...emptyKeyForm(), kind: 'openai' as const, name: 'mine', secret: 'sk-live-abcd1234' };
    expect(drawModels([], typed)).toContain('type="password"');
    const saved = drawModels([{ ...key, kind: 'openai', name: 'mine', base_url: null }], emptyKeyForm());
    expect(saved).not.toContain('sk-live-abcd1234');
    expect(saved).toContain('…1234');
  });
  it('a Bedrock key is its access keys as JSON and its region', () => {
    const form = { ...emptyKeyForm(), kind: 'bedrock' as const, name: 'aws', access_key_id: 'AKIA1', secret: 's3cr3t', region: 'eu-west-1' };
    expect(keyBody(form)).toEqual({ kind: 'bedrock', name: 'aws', secret: JSON.stringify({ access_key_id: 'AKIA1', secret_access_key: 's3cr3t' }), base_url: 'eu-west-1' });
  });
});
