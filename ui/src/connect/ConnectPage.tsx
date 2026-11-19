import { Check, PlugZap } from 'lucide-react';
import { type FormEvent, useRef, useState } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { checkAgent } from '../api';
import { Button } from '../components/ui/button';
import { CodeBlock } from '../components/ui/code-block';
import { Field, Input, Textarea } from '../components/ui/input';
import { PageBody } from '../components/ui/page';
import { Segmented } from '../components/ui/segmented';
import { cn } from '@/lib/cn';
import type { AgentSpec } from '@/model/types';
import {
  type ConnectForm, type ConnectFrom, type ConnectKind, type ConnectReturn, checkBody, connectedAgent, connectedTeam, emptyForm, formProblem, saveConnected,
} from './connect';
import { type Connection, type ConnectionTest, codeBody, codeTeam, createConnection, deleteConnection, testConnection, testLine } from './connections';
import { errorMessage } from '@/lib/errors';

/** Where the test stands: not run (or the form changed since), running, connected, or why not. */
export type TestState =
  | { state: 'idle' }
  | { state: 'testing' }
  | { state: 'ok'; agent: AgentSpec; ms: number }
  | { state: 'error'; error: string }
  /** A Code connection, saved and tested: what its driver's agents did. */
  | { state: 'code'; result: ConnectionTest; connection: Connection };

const KINDS: { id: ConnectKind; label: string }[] = [{ id: 'http', label: 'HTTP' }, { id: 'bedrock', label: 'Bedrock' }, { id: 'code', label: 'Code' }];

/** A problem worth showing before a test: only once something is typed (an empty form just waits). */
const typed = (f: ConnectForm) => ({ http: f.url || f.headers, bedrock: f.model, code: f.command || f.env })[f.kind];
const shownProblem = (f: ConnectForm) => (typed(f) ? formProblem(f) : null);

/** The one line under the form: testing, connected (name and round trip), or the plain reason in alarm ink. */
function TestLine({ test, problem }: { test: TestState; problem: string | null }) {
  const base = 'min-h-5 text-sm';
  if (test.state === 'testing') return <p role="status" className={cn(base, 'text-muted')}>◐ Testing the connection…</p>;
  if (test.state === 'ok') {
    return (
      <p role="status" className={base}>
        ✓ Connected to <span className="font-medium">{test.agent.name}</span>
        <span className="text-muted" title="Round trip"> · {Math.round(test.ms)} ms</span>
      </p>
    );
  }
  if (test.state === 'code') return <CodeTestLine t={test.result} />;
  const why = test.state === 'error' ? test.error : problem;
  return <p role="status" className={cn(base, 'font-medium break-words text-alarm')}>{why ? `⚠ ${why}` : ''}</p>;
}

/** A Code connection's test: what its agents did, or why it didn't work and the end of the driver's stderr. */
function CodeTestLine({ t }: { t: ConnectionTest }) {
  if (t.ok) return <p role="status" className="min-h-5 text-sm">✓ {testLine(t)}</p>;
  return (
    <div role="status" className="flex flex-col gap-1 text-sm">
      <p className="font-medium break-words text-alarm">⚠ {t.error ?? 'The test run did not work.'} <span className="font-normal text-muted">· {testLine(t)}</span></p>
      {t.stderr_tail.trim() && <CodeBlock className="max-h-40 rounded whitespace-pre-wrap">{t.stderr_tail.trim().slice(-2000)}</CodeBlock>}
    </div>
  );
}

/** A Code connection's fields: its name, the command, the folder it runs in and its environment. */
function CodeFields({ form, set, busy }: { form: ConnectForm; set: (p: Partial<ConnectForm>) => void; busy: boolean }) {
  return (
    <>
      <p className="text-xs text-muted">Your driver runs on this computer: Shakedown starts this command for each run, with the run&apos;s ids in its environment, and your driver registers its agents and drives them.</p>
      <Field label="Name">
        <Input value={form.name} onChange={(e) => set({ name: e.target.value })} disabled={busy} autoComplete="off" placeholder="Our LangGraph team" className="h-8 text-sm" />
      </Field>
      <Field label="Command">
        <Input value={form.command} onChange={(e) => set({ command: e.target.value })} disabled={busy} spellCheck={false} autoComplete="off" aria-label="Command"
          placeholder="uv run python examples/drivers/langgraph_driver.py" className="h-8 font-mono text-xs" />
      </Field>
      <Field label={<span>Folder <span className="font-normal text-faint">· Optional</span></span>}>
        <Input value={form.workdir} onChange={(e) => set({ workdir: e.target.value })} disabled={busy} spellCheck={false} autoComplete="off" placeholder="/path/to/your/agents" className="h-8 font-mono text-xs" />
      </Field>
      <SecretLines label="Environment" value={form.env} onChange={(env) => set({ env })} disabled={busy} placeholder="ARENA_MODEL=gpt-4.1-mini" aria="Environment, one NAME=value per line"
        tip="Given to your driver. Model keys stay in Shakedown: the agents' gateway holds them." />
    </>
  );
}

/** The headers box: plain while it has focus, masked otherwise, so the values never sit on screen. */
function SecretLines({ label, tip, value, onChange, disabled, placeholder, aria }: {
  label: string; tip: string; value: string; onChange: (v: string) => void; disabled: boolean; placeholder: string; aria: string;
}) {
  const [focused, setFocused] = useState(false);
  return (
    <Field label={<span title={tip}>{label} <span className="font-normal text-faint">· Optional</span></span>}>
      <Textarea rows={3} value={value} onChange={(e) => onChange(e.target.value)} disabled={disabled} spellCheck={false} autoComplete="off" autoCapitalize="off"
        onFocus={() => setFocused(true)} onBlur={() => setFocused(false)} placeholder={placeholder} aria-label={aria}
        className={cn('resize-none', !focused && value && '[-webkit-text-security:disc]')} />
    </Field>
  );
}

function HeadersField({ value, onChange, disabled }: { value: string; onChange: (v: string) => void; disabled: boolean }) {
  return (
    <SecretLines label="Headers" value={value} onChange={onChange} disabled={disabled} placeholder="Authorization: Bearer …" aria="Headers, one Name: value per line"
      tip="Sent with every call to your agent. Shakedown keeps them in memory only and never shows them again." />
  );
}

function Fields({ form, set, busy }: { form: ConnectForm; set: (p: Partial<ConnectForm>) => void; busy: boolean }) {
  if (form.kind === 'code') return <CodeFields form={form} set={set} busy={busy} />;
  if (form.kind === 'bedrock') {
    return (
      <div className="grid gap-4 sm:grid-cols-[1fr_9rem]">
        <Field label="Model">
          <Input value={form.model} onChange={(e) => set({ model: e.target.value })} disabled={busy} spellCheck={false} autoComplete="off"
            placeholder="anthropic.claude-3-5-haiku-20241022-v1:0" className="h-8 font-mono text-xs" />
        </Field>
        <Field label={<span title="Called with this machine's AWS credentials">Region</span>}>
          <Input value={form.region} onChange={(e) => set({ region: e.target.value })} disabled={busy} spellCheck={false} autoComplete="off"
            placeholder="us-east-1" className="h-8 font-mono text-xs" />
        </Field>
      </div>
    );
  }
  return (
    <>
      <Field label={<span title="Shakedown sends it an arena-agent/1 hello, then the agent's turns">URL</span>}>
        <Input type="url" inputMode="url" value={form.url} onChange={(e) => set({ url: e.target.value })} disabled={busy} spellCheck={false} autoComplete="off"
          placeholder="https://agent.example.com/arena" className="h-8 font-mono text-xs" />
      </Field>
      <HeadersField value={form.headers} onChange={(headers) => set({ headers })} disabled={busy} />
    </>
  );
}

/** The screen itself, drawn from its state: the choice, the fields, the test line and the one primary action. */
export function ConnectView({ form, test, onChange, onSubmit, onCancel }: {
  form: ConnectForm; test: TestState; onChange: (f: ConnectForm) => void; onSubmit: () => void; onCancel: () => void;
}) {
  const busy = test.state === 'testing';
  const set = (p: Partial<ConnectForm>) => onChange({ ...form, ...p });
  const submit = (e: FormEvent) => { e.preventDefault(); onSubmit(); };
  const ready = test.state === 'ok' || (test.state === 'code' && test.result.ok);
  const testable = form.kind === 'code' ? !!codeBody(form) : !!checkBody(form);
  return (
    <PageBody width="narrow" className="max-w-xl gap-6 pt-10">
      <h1 className="text-xl font-semibold tracking-tight">Connect your agent</h1>
      <form onSubmit={submit} className="flex flex-col gap-4 rounded-lg border border-line bg-surface p-4 shadow-panel sm:p-6" noValidate>
        <Segmented label="Where your agent runs" value={form.kind} options={KINDS} disabled={busy} onChange={(kind) => set({ kind })} />
        <Fields form={form} set={set} busy={busy} />
        <TestLine test={test} problem={shownProblem(form)} />
        <div className="flex flex-wrap items-center gap-2">
          <Button type="submit" variant="primary" className="h-9 min-w-40 px-5 text-sm" disabled={busy || (!ready && !testable)}>
            {ready ? <><Check />{form.kind === 'code' ? 'Use this connection' : 'Use this agent'}</> : <><PlugZap />{busy ? 'Testing…' : 'Test connection'}</>}
          </Button>
          <Button type="button" variant="ghost" className="h-9 text-muted" onClick={onCancel}>Cancel</Button>
        </div>
      </form>
    </PageBody>
  );
}

/** The form and its test; any change after a test means testing again. */
function useConnect() {
  const [form, setForm] = useState<ConnectForm>(() => emptyForm());
  const [test, setTest] = useState<TestState>({ state: 'idle' });
  const seq = useRef(0);
  const change = (f: ConnectForm) => { seq.current++; setForm(f); setTest({ state: 'idle' }); };
  // A Code connection saved for a test the form has since changed from: dropped when another is saved.
  const unused = useRef<string | null>(null);
  const check = async (): Promise<TestState | null> => {
    if (form.kind !== 'code') {
      const body = checkBody(form);
      if (!body) return null;
      const a = await checkAgent(body);
      return a.ok ? { state: 'ok', agent: connectedAgent(a), ms: a.ms } : { state: 'error', error: a.error };
    }
    const body = codeBody(form);
    if (!body) return null;
    if (unused.current) void deleteConnection(unused.current).catch(() => {});
    const connection = await createConnection(body);
    unused.current = connection.id;
    return { state: 'code', connection, result: await testConnection(connection.id) };
  };
  const run = async () => {
    const mine = ++seq.current;
    setTest({ state: 'testing' });
    let next: TestState | null;
    try {
      next = await check();
    } catch (e) {
      next = { state: 'error', error: errorMessage(e) };
    }
    if (mine === seq.current) setTest(next ?? { state: 'idle' });
  };
  const used = () => { unused.current = null; };
  return { form, test, change, run, used };
}

/** Connect your agent (`/connect`): an HTTP endpoint or a Bedrock model, tested, then used where Connect was opened from. */
export default function ConnectPage() {
  const nav = useNavigate();
  const from = (useLocation().state ?? {}) as Partial<ConnectFrom>;
  const back = from.from ?? '/experiments/new';
  const c = useConnect();
  const give = (r: ConnectReturn) => nav(back, { state: r });
  const submit = () => {
    if (c.test.state === 'code' && c.test.result.ok) {
      c.used();
      give({ connected: codeTeam(c.test.connection), setup: from.setup, form: from.form });
      return;
    }
    if (c.test.state !== 'ok') { void c.run(); return; }
    saveConnected(c.test.agent);
    give({ connected: connectedTeam(c.test.agent), setup: from.setup, form: from.form });
  };
  return <ConnectView form={c.form} test={c.test} onChange={c.change} onSubmit={submit} onCancel={() => give({ setup: from.setup, form: from.form })} />;
}
