// Settings → Models: your own model keys and OpenAI-compatible servers (vLLM, Ollama, a RunPod
// pod). A key is typed once and never shown again: the list shows its last four characters.
// Test makes a one-token call with it.
import { useState } from 'react';
import { addKey, deleteKey, type KeyKind, type KeyRow, listKeys, type NewKey, testKey } from '@/api/models';
import { Button } from '@/components/ui/button';
import { Field, Input } from '@/components/ui/input';
import { Panel, PanelHeader } from '@/components/ui/panel';
import { Segmented } from '@/components/ui/segmented';
import { EmptyState, ErrorState } from '@/components/ui/state';
import { useAction } from '@/hooks/useAction';
import { checkProviders } from '@/hooks/useProviders';
import { useQuery } from '@/hooks/useQuery';
import SettingsPage from './SettingsPage';

const KINDS: readonly { id: KeyKind; label: string }[] = [
  { id: 'custom', label: 'Your server' },
  { id: 'openrouter', label: 'OpenRouter' },
  { id: 'openai', label: 'OpenAI' },
  { id: 'anthropic', label: 'Anthropic' },
  { id: 'bedrock', label: 'Bedrock' },
];

export interface KeyForm { kind: KeyKind; name: string; secret: string; base_url: string; model: string; access_key_id: string; region: string }

export const emptyKeyForm = (): KeyForm => ({ kind: 'custom', name: '', secret: '', base_url: '', model: '', access_key_id: '', region: 'us-east-1' });

/** What `POST /api/models/keys` gets for `f`. */
export function keyBody(f: KeyForm): NewKey {
  const base = { kind: f.kind, name: f.name.trim() };
  const model = f.model.trim() ? { model: f.model.trim() } : {};
  if (f.kind === 'bedrock') {
    const secret = JSON.stringify({ access_key_id: f.access_key_id.trim(), secret_access_key: f.secret });
    return { ...base, secret, base_url: f.region.trim(), ...model };
  }
  const url = f.base_url.trim() ? { base_url: f.base_url.trim() } : {};
  return { ...base, secret: f.secret, ...url, ...model };
}

/** A key's last Test, in a few words. */
export function checkLine(k: KeyRow): string {
  if (k.checked_ok === null) return 'Not tested';
  return k.checked_ok ? 'Works' : `⚠ ${k.check_detail}`;
}

function KindFields({ form, set }: { form: KeyForm; set: (p: Partial<KeyForm>) => void }) {
  if (form.kind === 'bedrock') {
    return (
      <>
        <Field label="Access key ID"><Input value={form.access_key_id} onChange={(e) => set({ access_key_id: e.target.value })} /></Field>
        <Field label="Secret access key"><Input type="password" autoComplete="off" value={form.secret} onChange={(e) => set({ secret: e.target.value })} /></Field>
        <Field label="Region"><Input value={form.region} onChange={(e) => set({ region: e.target.value })} /></Field>
      </>
    );
  }
  const custom = form.kind === 'custom';
  return (
    <>
      {custom && <Field label="Server URL"><Input value={form.base_url} placeholder="http://127.0.0.1:8000/v1" onChange={(e) => set({ base_url: e.target.value })} /></Field>}
      <Field label={custom ? 'Key (if the server needs one)' : 'Key'}>
        <Input type="password" autoComplete="off" value={form.secret} onChange={(e) => set({ secret: e.target.value })} />
      </Field>
    </>
  );
}

function KeyList({ keys, testing, onTest, onDelete }: { keys: KeyRow[]; testing: string | null; onTest: (id: string) => void; onDelete: (id: string) => void }) {
  if (keys.length === 0) return <EmptyState title="No keys yet." hint="Agents use the keys in shaked's environment until you add one." />;
  return (
    <ul className="divide-y divide-line">
      {keys.map((k) => (
        <li key={k.id} className="flex flex-wrap items-center gap-3 px-4 py-2 text-sm">
          <span className="font-medium">{k.name}</span>
          <span className="text-xs text-muted">{KINDS.find((x) => x.id === k.kind)?.label ?? k.kind}</span>
          {k.key_last4 && <code className="font-mono text-xs text-muted">…{k.key_last4}</code>}
          <span className={k.checked_ok === false ? 'text-xs text-alarm' : 'text-xs text-muted'}>{checkLine(k)}</span>
          <span className="flex-1" />
          <Button variant="ghost" size="sm" disabled={testing === k.id} onClick={() => onTest(k.id)}>{testing === k.id ? 'Testing…' : 'Test'}</Button>
          <Button variant="ghost" size="sm" onClick={() => onDelete(k.id)}>Delete</Button>
        </li>
      ))}
    </ul>
  );
}

export function ModelsView({ keys, form, testing, error, onChange, onAdd, onTest, onDelete }: {
  keys: KeyRow[]; form: KeyForm; testing: string | null; error: string | null;
  onChange: (f: KeyForm) => void; onAdd: () => void; onTest: (id: string) => void; onDelete: (id: string) => void;
}) {
  const set = (p: Partial<KeyForm>) => onChange({ ...form, ...p });
  return (
    <>
      <Panel>
        <PanelHeader title="Add a key or server" />
        <form className="flex flex-col gap-3 p-4" onSubmit={(e) => { e.preventDefault(); onAdd(); }}>
          <Segmented label="Kind" value={form.kind} options={KINDS} onChange={(kind) => set({ kind })} />
          <Field label="Name" hint={form.kind === 'custom' ? `Agents pick it as custom:${form.name.trim() || '<name>'}` : undefined}>
            <Input value={form.name} placeholder="my-key" onChange={(e) => set({ name: e.target.value })} />
          </Field>
          <KindFields form={form} set={set} />
          <Field label="Model (optional, for Test)"><Input value={form.model} onChange={(e) => set({ model: e.target.value })} /></Field>
          {error && <p className="text-xs text-alarm">⚠ {error}</p>}
          <Button type="submit" disabled={!form.name.trim()} className="self-start">Save</Button>
        </form>
      </Panel>
      <Panel>
        <PanelHeader title="Your keys" />
        <KeyList keys={keys} testing={testing} onTest={onTest} onDelete={onDelete} />
      </Panel>
    </>
  );
}

export default function ModelsPage() {
  const keys = useQuery('model-keys', listKeys);
  // The provider list (status bar, agent forms) applies these keys: ask for it again.
  const changed = () => { keys.reload(); checkProviders().catch(() => {}); };
  const [form, setForm] = useState<KeyForm>(emptyKeyForm);
  const [testing, setTesting] = useState<string | null>(null);
  const add = useAction(async () => {
    await addKey(keyBody(form));
    setForm({ ...emptyKeyForm(), kind: form.kind });
    changed();
  });
  const test = useAction(async (id: string) => {
    setTesting(id);
    try { await testKey(id); } finally { setTesting(null); keys.reload(); }
  });
  const remove = useAction(async (id: string) => { await deleteKey(id); changed(); });
  const failed = keys.error ?? test.error ?? remove.error;
  return (
    <SettingsPage>
      {failed && <ErrorState error={String(failed)} />}
      <ModelsView keys={keys.data ?? []} form={form} testing={testing} error={add.error ? String(add.error) : null} onChange={setForm}
        onAdd={() => { void add.run(); }} onTest={(id) => { void test.run(id); }} onDelete={(id) => { void remove.run(id); }} />
    </SettingsPage>
  );
}
