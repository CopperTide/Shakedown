// Settings → Observability: send every run as an OpenTelemetry trace to Langfuse, Arize Phoenix or
// any OTLP collector (Jaeger, Grafana Tempo, Honeycomb). Keys and headers are typed once and
// never shown again. Test sends one span; the exports list says what went where.
import { useState } from 'react';
import {
  addTarget, deleteTarget, type ExportRow, listExports, listTargets, type NewTarget, setTargetFlags,
  type TargetFlags, type TargetKind, type TargetRow, testTarget,
} from '@/api/observability';
import { Button } from '@/components/ui/button';
import { Field, Input, Textarea } from '@/components/ui/input';
import { Panel, PanelHeader } from '@/components/ui/panel';
import { Segmented } from '@/components/ui/segmented';
import { EmptyState, ErrorState } from '@/components/ui/state';
import { Toggle } from '@/components/ui/toggle';
import { useAction } from '@/hooks/useAction';
import { useQuery } from '@/hooks/useQuery';
import SettingsPage from './SettingsPage';

export const KINDS: readonly { id: TargetKind; label: string }[] = [
  { id: 'langfuse', label: 'Langfuse' },
  { id: 'phoenix', label: 'Arize Phoenix' },
  { id: 'otlp', label: 'OTLP collector' },
];

const REGIONS: readonly { id: 'us' | 'eu' | 'self'; label: string }[] = [
  { id: 'us', label: 'Cloud US' },
  { id: 'eu', label: 'Cloud EU' },
  { id: 'self', label: 'Your server' },
];

export interface TargetForm {
  kind: TargetKind; name: string; region: 'us' | 'eu' | 'self'; base_url: string;
  public_key: string; secret_key: string; api_key: string; project: string;
  endpoint: string; headers: string; include_content: boolean | null;
}

export const emptyTargetForm = (): TargetForm => ({
  kind: 'langfuse', name: '', region: 'us', base_url: '', public_key: '', secret_key: '',
  api_key: '', project: '', endpoint: '', headers: '', include_content: null,
});

/** `name: value` lines as headers; blank lines and lines without a colon are left out. */
export function parseHeaders(text: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const line of text.split('\n')) {
    const at = line.indexOf(':');
    if (at > 0) out[line.slice(0, at).trim()] = line.slice(at + 1).trim();
  }
  return out;
}

const trimmed = (v: string) => (v.trim() ? v.trim() : undefined);

/** What `POST /api/observability/targets` gets for `f`: only the fields its preset reads. */
export function targetBody(f: TargetForm): NewTarget {
  const base: NewTarget = { kind: f.kind, name: f.name.trim(), ...(f.include_content === null ? {} : { include_content: f.include_content }) };
  if (f.kind === 'langfuse') {
    const url = f.region === 'self' ? { base_url: trimmed(f.base_url) } : {};
    return { ...base, region: f.region, ...url, public_key: f.public_key.trim(), secret_key: f.secret_key };
  }
  if (f.kind === 'phoenix') {
    return { ...base, base_url: trimmed(f.base_url), api_key: trimmed(f.api_key), project: trimmed(f.project) };
  }
  return { ...base, endpoint: f.endpoint.trim(), headers: parseHeaders(f.headers) };
}

/** A target's last Test, in a few words. */
export function checkLine(t: TargetRow): string {
  if (t.checked_ok === null) return 'Not tested';
  return t.checked_ok ? 'Works' : `⚠ ${t.check_detail}`;
}

function PresetFields({ form, set }: { form: TargetForm; set: (p: Partial<TargetForm>) => void }) {
  const secret = (label: string, key: 'secret_key' | 'api_key') => (
    <Field label={label}>
      <Input type="password" autoComplete="off" value={form[key]}
        onChange={(e) => set(key === 'secret_key' ? { secret_key: e.target.value } : { api_key: e.target.value })} />
    </Field>
  );
  if (form.kind === 'langfuse') {
    return (
      <>
        <Segmented label="Where" value={form.region} options={REGIONS} onChange={(region) => set({ region })} />
        {form.region === 'self' && <Field label="Langfuse URL"><Input value={form.base_url} placeholder="https://langfuse.example.com" onChange={(e) => set({ base_url: e.target.value })} /></Field>}
        <Field label="Public key"><Input value={form.public_key} placeholder="pk-lf-…" onChange={(e) => set({ public_key: e.target.value })} /></Field>
        {secret('Secret key', 'secret_key')}
      </>
    );
  }
  if (form.kind === 'phoenix') {
    return (
      <>
        <Field label="Phoenix URL" hint="Your Phoenix (http://127.0.0.1:6006 when blank), or your Phoenix Cloud space's URL.">
          <Input value={form.base_url} placeholder="http://127.0.0.1:6006" onChange={(e) => set({ base_url: e.target.value })} />
        </Field>
        {secret('API key (if it needs one)', 'api_key')}
        <Field label="Project"><Input value={form.project} placeholder="shakedown" onChange={(e) => set({ project: e.target.value })} /></Field>
      </>
    );
  }
  return (
    <>
      <Field label="Collector URL" hint="OTLP/HTTP: /v1/traces is added unless the URL ends with it.">
        <Input value={form.endpoint} placeholder="http://127.0.0.1:4318" onChange={(e) => set({ endpoint: e.target.value })} />
      </Field>
      <Field label="Headers (one name: value a line)">
        <Textarea rows={3} autoComplete="off" value={form.headers} placeholder="x-honeycomb-team: …" onChange={(e) => set({ headers: e.target.value })} />
      </Field>
    </>
  );
}

function TargetItem({ t, testing, onTest, onFlags, onDelete }: {
  t: TargetRow; testing: boolean; onTest: () => void; onFlags: (f: Partial<TargetFlags>) => void; onDelete: () => void;
}) {
  return (
    <li className="flex flex-col gap-2 px-4 py-3 text-sm">
      <div className="flex flex-wrap items-center gap-3">
        <span className="font-medium">{t.name}</span>
        <span className="text-xs text-muted">{KINDS.find((k) => k.id === t.kind)?.label ?? t.kind}</span>
        <code className="truncate font-mono text-xs text-muted">{t.endpoint}</code>
        <span className={t.checked_ok === false ? 'text-xs text-alarm' : 'text-xs text-muted'}>{checkLine(t)}</span>
        <span className="flex-1" />
        <Button variant="ghost" size="sm" disabled={testing} onClick={onTest}>{testing ? 'Testing…' : 'Test'}</Button>
        <Button variant="ghost" size="sm" onClick={onDelete}>Delete</Button>
      </div>
      <div className="flex flex-wrap gap-6">
        <Toggle label="Every run" description="Sent when it ends" checked={t.auto_export} onChange={(on) => onFlags({ auto_export: on })} />
        <Toggle label="Content" description="Prompts, completions, tool arguments" checked={t.include_content} onChange={(on) => onFlags({ include_content: on })} />
        {t.kind === 'otlp' && <Toggle label="Metrics" description="OTLP metrics every minute" checked={t.export_metrics} onChange={(on) => onFlags({ export_metrics: on })} />}
      </div>
    </li>
  );
}

function ExportList({ exports }: { exports: ExportRow[] }) {
  if (exports.length === 0) return <EmptyState title="Nothing exported yet." hint="Runs are sent when they end." />;
  return (
    <ul className="divide-y divide-line">
      {exports.map((x) => (
        <li key={`${x.target_id}-${x.branch_id}`} className="flex flex-wrap items-center gap-3 px-4 py-2 text-xs">
          <code className="font-mono text-muted">{x.branch_id.slice(0, 8)}</code>
          <span>{x.target}</span>
          <span className={x.state === 'failed' ? 'text-alarm' : 'text-muted'}>
            {x.state === 'done' ? `${x.spans ?? 0} spans` : x.state}{x.last_error ? ` · ${x.last_error}` : ''}
          </span>
        </li>
      ))}
    </ul>
  );
}

export function ObservabilityView({ targets, exports, form, testing, error, onChange, onAdd, onTest, onFlags, onDelete }: {
  targets: TargetRow[]; exports: ExportRow[]; form: TargetForm; testing: string | null; error: string | null;
  onChange: (f: TargetForm) => void; onAdd: () => void; onTest: (id: string) => void;
  onFlags: (id: string, f: Partial<TargetFlags>) => void; onDelete: (id: string) => void;
}) {
  const set = (p: Partial<TargetForm>) => onChange({ ...form, ...p });
  return (
    <>
      <Panel>
        <PanelHeader title="Send runs to" />
        <form className="flex flex-col gap-3 p-4" onSubmit={(e) => { e.preventDefault(); onAdd(); }}>
          <Segmented label="Kind" value={form.kind} options={KINDS} onChange={(kind) => set({ kind })} />
          <Field label="Name"><Input value={form.name} placeholder={form.kind} onChange={(e) => set({ name: e.target.value })} /></Field>
          <PresetFields form={form} set={set} />
          {error && <p className="text-xs text-alarm">⚠ {error}</p>}
          <Button type="submit" disabled={!form.name.trim()} className="self-start">Save</Button>
        </form>
      </Panel>
      <Panel>
        <PanelHeader title="Targets" />
        {targets.length === 0 ? <EmptyState title="No targets yet." hint="Prometheus can also scrape /metrics." /> : (
          <ul className="divide-y divide-line">
            {targets.map((t) => (
              <TargetItem key={t.id} t={t} testing={testing === t.id} onTest={() => onTest(t.id)} onFlags={(f) => onFlags(t.id, f)} onDelete={() => onDelete(t.id)} />
            ))}
          </ul>
        )}
      </Panel>
      <Panel>
        <PanelHeader title="Recent exports" />
        <ExportList exports={exports} />
      </Panel>
    </>
  );
}

export default function ObservabilityPage() {
  const targets = useQuery('observability-targets', listTargets);
  const exports = useQuery('observability-exports', listExports);
  const [form, setForm] = useState<TargetForm>(emptyTargetForm);
  const [testing, setTesting] = useState<string | null>(null);
  const add = useAction(async () => {
    await addTarget(targetBody(form));
    setForm({ ...emptyTargetForm(), kind: form.kind });
    targets.reload();
  });
  const test = useAction(async (id: string) => {
    setTesting(id);
    try { await testTarget(id); } finally { setTesting(null); targets.reload(); }
  });
  const flags = useAction(async (id: string, f: Partial<TargetFlags>) => { await setTargetFlags(id, f); targets.reload(); });
  const remove = useAction(async (id: string) => { await deleteTarget(id); targets.reload(); exports.reload(); });
  const failed = targets.error ?? exports.error ?? test.error ?? flags.error ?? remove.error;
  return (
    <SettingsPage>
      {failed && <ErrorState error={String(failed)} />}
      <ObservabilityView targets={targets.data ?? []} exports={exports.data ?? []} form={form} testing={testing}
        error={add.error ? String(add.error) : null} onChange={setForm} onAdd={() => { void add.run(); }}
        onTest={(id) => { void test.run(id); }} onFlags={(id, f) => { void flags.run(id, f); }} onDelete={(id) => { void remove.run(id); }} />
    </SettingsPage>
  );
}
