// Importing a git repo: a URL or a local path and a ref, Check shows what was found and what
// happens to each piece, Import installs, registers and writes it.
import { GitBranch } from 'lucide-react';
import { type ReactNode, useState } from 'react';
import { type GitImport, importGit } from '@/api/project';
import { Button } from '@/components/ui/button';
import { Dialog, DialogActions, DialogContent } from '@/components/ui/dialog';
import { Hint } from '@/components/ui/hint';
import { Field, Input } from '@/components/ui/input';
import { errorMessage } from '@/lib/errors';

const ACTION: Record<GitImport['items'][number]['action'], string> = {
  write: 'new',
  replace: 'replaces the last import',
  same: 'already the same',
  keep: 'kept',
  remove: 'removed (to the trash)',
};

function Section({ title, hint, children }: { title: string; hint?: string; children: ReactNode }) {
  return <section className="grid gap-1"><h4 className="text-2xs font-medium text-muted"><Hint hint={hint}>{title}</Hint></h4>{children}</section>;
}

/** What the repo holds and what importing it does. */
export function ImportPlan({ plan }: { plan: GitImport }) {
  const issues = [...plan.conflicts, ...plan.problems, ...(plan.project?.problems ?? [])];
  return (
    <div className="grid gap-3">
      <p className="text-xs text-muted">
        <span className="font-mono">{plan.commit.slice(0, 12)}</span> · {plan.ref}
        {plan.project && <> · project <span className="text-fg">{plan.project.name}</span></>}
        {plan.previous_commit && plan.previous_commit !== plan.commit && <> · was <span className="font-mono">{plan.previous_commit.slice(0, 12)}</span></>}
      </p>
      {issues.length > 0 && <ul role="alert" className="grid gap-1 text-alarm">{issues.map((m) => <li key={m}>{m}</li>)}</ul>}
      <Section title={`Packages (${plan.packages.length})`}>
        {plan.packages.length ? <ul className="grid gap-0.5">{plan.packages.map((p) => <li key={p.name}>{p.name} <span className="text-faint">· {p.kind} · <span className="font-mono text-2xs">{p.dir}</span>{p.python ? ' · Python (uv)' : ''}</span></li>)}</ul> : <p className="text-faint">No packages.</p>}
      </Section>
      <Section title={`Project files (${plan.items.length})`}>
        {plan.items.length ? <ul className="grid gap-0.5">{plan.items.map((i) => <li key={`${i.kind}/${i.id}`} title={i.reason ?? undefined}>{i.label} <span className="text-faint">· {i.kind} · {ACTION[i.action]}</span></li>)}</ul> : <p className="text-faint"><Hint hint="The repo is not a project folder: no worlds, scenarios, setups or experiments.">None</Hint></p>}
      </Section>
      {plan.views.length > 0 && <Section title="Views"><ul className="grid gap-0.5">{plan.views.map((v) => <li key={`${v.world}/${v.file}`}>{v.world} <span className="font-mono text-2xs text-faint">{v.file}</span></li>)}</ul></Section>}
      {plan.compose.length > 0 && (
        <Section title="Compose files" hint="Candidates for the service integration.">
          <ul className="grid gap-0.5">{plan.compose.map((c) => <li key={c.file}><span className="font-mono text-2xs">{c.file}</span> <span className="text-faint">· {c.services.join(', ')}</span></li>)}</ul>
        </Section>
      )}
      {plan.notes.length > 0 && (
        <details className="text-xs text-muted">
          <summary className="cursor-pointer">{`Notes (${plan.notes.length})`}</summary>
          <ul className="mt-1 grid gap-0.5">{plan.notes.map((n) => <li key={n}>{n}</li>)}</ul>
        </details>
      )}
    </div>
  );
}

/** The dialog's primary button: Check, then Import (blocked by a taken package name), then Done. */
function primary(plan: GitImport | null, done: GitImport | null, busy: boolean): { label: string; blocked: boolean } {
  if (done) return { label: 'Done', blocked: false };
  if (busy) return { label: plan ? 'Importing…' : 'Checking…', blocked: true };
  return { label: plan ? 'Import' : 'Check', blocked: plan != null && plan.conflicts.length > 0 };
}

function Imported({ done }: { done: GitImport }) {
  const how = done.registered ? 'registered' : 'installed';
  const removed = done.removed?.length ? ` · ${done.removed.length} removed` : '';
  const hint = `${done.registered ? '' : 'Packages are registered at the next start. '}Files are written into the project${done.removed?.length ? '; files no longer in the repo went to the trash' : ''}.`;
  return <p role="status" className="text-xs"><Hint hint={hint}>{`✓ Imported · ${done.packages.length} packages ${how}${removed}`}</Hint></p>;
}

export default function ImportFromGit({ onImported }: { onImported: () => void }) {
  const [open, setOpen] = useState(false);
  const [source, setSource] = useState({ url: '', ref: '' });
  const [plan, setPlan] = useState<GitImport | null>(null);
  const [done, setDone] = useState<GitImport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const reset = (o: boolean) => { setOpen(o); setPlan(null); setDone(null); setError(null); };
  const edit = (next: Partial<typeof source>) => { setSource({ ...source, ...next }); setPlan(null); setDone(null); };
  const run = async (confirm: boolean) => {
    setBusy(true); setError(null);
    try {
      const out = await importGit(source.url.trim(), source.ref.trim(), confirm);
      if (confirm) { setDone(out); onImported(); } else setPlan(out);
    } catch (e) { setError(errorMessage(e)); } finally { setBusy(false); }
  };
  const go = () => { if (done) reset(false); else void run(plan != null); };
  const { label, blocked } = primary(plan, done, busy);
  const shown = done ?? plan;
  return (
    <Dialog open={open} onOpenChange={reset}>
      <Button onClick={() => setOpen(true)}><GitBranch />Import from git</Button>
      <DialogContent title="Import from git" hint="Brings in a repo's packages, worlds, scenarios, setups and experiments as they are: its code runs on this machine.">
        <div className="grid max-h-[60vh] gap-3 overflow-auto px-4 py-3 text-sm">
          <div className="grid gap-3 sm:grid-cols-[minmax(0,3fr)_minmax(0,1fr)]">
            <Field label="Repository" hint="A git URL or a local path."><Input aria-label="Repository" value={source.url} onChange={(e) => edit({ url: e.target.value })} placeholder="https://github.com/acme/ops.git" className="font-mono" /></Field>
            <Field label="Ref"><Input aria-label="Ref" value={source.ref} onChange={(e) => edit({ ref: e.target.value })} placeholder="HEAD" className="font-mono" /></Field>
          </div>
          {done && <Imported done={done} />}
          {shown && <ImportPlan plan={shown} />}
          {error && <p role="alert" className="text-alarm">{error}</p>}
        </div>
        <DialogActions onCancel={() => reset(false)} onGo={go} disabled={blocked || !source.url.trim()} label={label} />
      </DialogContent>
    </Dialog>
  );
}
