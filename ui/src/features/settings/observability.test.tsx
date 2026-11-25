import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { TargetRow } from '@/api/observability';
import { checkLine, emptyTargetForm, ObservabilityView, parseHeaders, targetBody, type TargetForm } from './ObservabilityPage';

const target: TargetRow = {
  id: 't1', name: 'langfuse', kind: 'langfuse', endpoint: 'https://us.cloud.langfuse.com/api/public/otel/v1/traces', project: null,
  header_names: ['Authorization'], include_content: false, auto_export: true, export_metrics: false,
  checked_ok: true, checked_at: '2026-11-25T10:00:00Z', check_detail: '', created_at: '2026-11-25T10:00:00Z',
};
const draw = (form: TargetForm, targets: TargetRow[] = [target]) => renderToStaticMarkup(
  <ObservabilityView targets={targets} exports={[]} form={form} testing={null} error={null}
    onChange={() => {}} onAdd={() => {}} onTest={() => {}} onFlags={() => {}} onDelete={() => {}} />,
);

describe('Observability', () => {
  it('sends Langfuse its keys and region, and never shows a secret again', () => {
    const form = { ...emptyTargetForm(), name: ' lf ', public_key: ' pk-lf-1 ', secret_key: 'sk-lf-secret' };
    expect(targetBody(form)).toEqual({ kind: 'langfuse', name: 'lf', region: 'us', public_key: 'pk-lf-1', secret_key: 'sk-lf-secret' });
    expect(draw(form)).toContain('type="password"');
    const saved = draw(emptyTargetForm());
    expect(saved).not.toContain('sk-lf-secret');
    expect(saved).toContain('us.cloud.langfuse.com');
    expect(checkLine(target)).toBe('Works');
  });
  it('sends Phoenix only what is filled in', () => {
    const form = { ...emptyTargetForm(), kind: 'phoenix' as const, name: 'px', project: 'support' };
    expect(targetBody(form)).toEqual({ kind: 'phoenix', name: 'px', base_url: undefined, api_key: undefined, project: 'support' });
  });
  it('reads a generic collector\'s headers a line each', () => {
    expect(parseHeaders('x-honeycomb-team: abc\n\nbad line\nAuthorization: Bearer a:b')).toEqual({ 'x-honeycomb-team': 'abc', Authorization: 'Bearer a:b' });
    const form = { ...emptyTargetForm(), kind: 'otlp' as const, name: 'jaeger', endpoint: 'http://127.0.0.1:4318', include_content: false };
    expect(targetBody(form)).toEqual({ kind: 'otlp', name: 'jaeger', endpoint: 'http://127.0.0.1:4318', headers: {}, include_content: false });
  });
  it('offers metrics only for a generic collector', () => {
    expect(draw(emptyTargetForm())).not.toContain('OTLP metrics every minute');
    expect(draw(emptyTargetForm(), [{ ...target, kind: 'otlp' }])).toContain('OTLP metrics every minute');
  });
});
