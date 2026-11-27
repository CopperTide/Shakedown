// Installing a runtime: from a git URL (optionally one folder of the repo) or an uploaded archive.
// Check shows what was found; Install installs its Python dependencies, registers its packages
// and runs their conformance check, whose result it shows. A runtime is code that runs here.
import { Download } from 'lucide-react';
import { type ReactNode, useState } from 'react';
import { type RuntimePlan, type RuntimeSource, base64Of, installRuntime } from '@/api/runtimes';
import { Button } from '@/components/ui/button';
import { Dialog, DialogActions, DialogContent } from '@/components/ui/dialog';
import { Field, Input } from '@/components/ui/input';
import { Segmented } from '@/components/ui/segmented';
import { errorMessage } from '@/lib/errors';
import CheckResult from './CheckResult';

/** The plain warning every install shows. */
export const CODE_WARNING = 'A runtime is code. Installing it runs its install steps, and every run runs its programs, on this machine with your access. Install only runtimes you trust.';

type Kind = RuntimeSource['kind'];
const KINDS = [{ id: 'git', label: 'Git URL' }, { id: 'archive', label: 'Archive' }] as const;

/** What an update starts from: the install's id, and its git source when it has one. */
export interface UpdateOf { id: string; url?: string; ref?: string; path?: string | null }

function Section({ title, children }: { title: string; children: ReactNode }) {
  return <section className="grid gap-1"><h4 className="text-2xs font-medium text-muted">{title}</h4>{children}</section>;
}

/** The packages found and what installing does; the checks once installed. */
export function RuntimePlanView({ plan }: { plan: RuntimePlan }) {
  const issues = [...plan.conflicts, ...plan.problems];
  const at = plan.source === 'archive' ? `${plan.url} · sha256 ${plan.commit.slice(0, 12)}` : `${plan.commit.slice(0, 12)} · ${plan.ref}`;
  return (
    <div className="grid gap-3">
      <p className="text-xs text-muted"><span className="font-mono">{at}</span>{plan.packages_only ? <> · folder <span className="font-mono">{plan.packages_only}</span></> : null} · id <span className="font-mono text-fg">{plan.id}</span></p>
      {issues.length > 0 && <ul role="alert" className="grid gap-1 text-alarm">{issues.map((m) => <li key={m}>{m}</li>)}</ul>}
      {plan.previous_commit && <p role="note" className="text-xs text-tone-warn">Updates the runtime {plan.id} installed before ({plan.previous_commit.slice(0, 12)}): its packages are replaced. To keep both, give this one another id.</p>}
      <Section title={`Packages (${plan.packages.length})`}>
        {plan.packages.length
          ? <ul className="grid gap-0.5">{plan.packages.map((p) => <li key={p.name}>{p.name} <span className="text-faint">· {p.kind} · <span className="font-mono text-2xs">{p.dir}</span>{p.python ? ' · Python dependencies installed with uv' : ''}</span></li>)}</ul>
          : <p className="text-faint">No packages (no shakedown-package.toml).</p>}
      </Section>
      {plan.notes.length > 0 && <ul className="grid gap-0.5 text-xs text-muted">{plan.notes.map((n) => <li key={n}>{n}</li>)}</ul>}
      {plan.checks && (
        <Section title="Conformance check">
          <div className="grid gap-2">{plan.checks.map((c) => <CheckResult key={c.package} check={c} />)}</div>
        </Section>
      )}
    </div>
  );
}

/** The source fields: a git URL, ref and folder, or an archive file and folder. */
/** The fields of the dialog besides the file: the repo, its ref, a folder of it and the install's id. */
type Fields = { url: string; ref: string; path: string; id: string };

function SourceFields({ kind, git, onGit, file, onFile, fixedId }: {
  kind: Kind; git: Fields; onGit: (g: Partial<Fields>) => void; file: File | null; onFile: (f: File | null) => void; fixedId?: string;
}) {
  const folder = (
    <>
      <Field label="Folder (optional)" hint="Only the packages under it"><Input aria-label="Folder" value={git.path} onChange={(e) => onGit({ path: e.target.value })} placeholder="packages/desk" className="font-mono" /></Field>
      {!fixedId && <Field label="Id (optional)" hint="Default: the repo's or archive's name"><Input aria-label="Install id" value={git.id} onChange={(e) => onGit({ id: e.target.value })} placeholder="desk-runtime" className="font-mono" /></Field>}
    </>
  );
  if (kind === 'archive') {
    return (
      <div className="grid gap-3 sm:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]">
        <Field label="Archive (.zip, .tar.gz)"><input aria-label="Archive" type="file" accept=".zip,.tar.gz,.tgz,application/zip,application/gzip" className="text-sm" onChange={(e) => onFile(e.target.files?.[0] ?? null)} /></Field>
        {folder}
        {file && <p className="text-xs text-muted sm:col-span-2">{file.name} · {Math.max(1, Math.round(file.size / 1024))} KB</p>}
      </div>
    );
  }
  return (
    <div className="grid gap-3 sm:grid-cols-[minmax(0,3fr)_minmax(0,1fr)]">
      <Field label="Repository (git URL or path)"><Input aria-label="Repository" value={git.url} onChange={(e) => onGit({ url: e.target.value })} placeholder="git@github.com:acme/desk-runtime.git" className="font-mono" /></Field>
      <Field label="Ref"><Input aria-label="Ref" value={git.ref} onChange={(e) => onGit({ ref: e.target.value })} placeholder="HEAD" className="font-mono" /></Field>
      <div className="grid gap-3 sm:col-span-2 sm:grid-cols-2">{folder}</div>
    </div>
  );
}

function label(plan: RuntimePlan | null, done: boolean, busy: boolean): string {
  if (done) return 'Done';
  if (busy) return plan ? 'Installing…' : 'Checking…';
  return plan ? 'Install' : 'Check';
}

/** The git fields an install starts with: an update's own, else empty. */
const gitOf = (u?: UpdateOf): Fields => ({ url: u?.url ?? '', ref: u?.ref ?? '', path: u?.path ?? '', id: '' });

/** The source the fields describe. */
async function sourceOf(kind: Kind, git: { url: string; ref: string }, file: File | null): Promise<RuntimeSource> {
  if (kind === 'git') return { kind: 'git', url: git.url.trim(), ref: git.ref.trim() };
  return { kind: 'archive', name: file?.name ?? '', data: file ? await base64Of(file) : '' };
}

/** A plan that can't be installed: a taken package name, or no package at all. */
const stuck = (plan: RuntimePlan | null, done: boolean) => plan != null && !done && (plan.conflicts.length > 0 || plan.packages.length === 0);

/** The dialog's state: the source asked for, the plan, then the install. */
function useInstall(update: UpdateOf | undefined, onInstalled: () => void) {
  const [kind, setKind] = useState<Kind>(update && !update.url ? 'archive' : 'git');
  const [git, setGit] = useState(gitOf(update));
  const [file, setFile] = useState<File | null>(null);
  const [plan, setPlan] = useState<RuntimePlan | null>(null);
  const [done, setDone] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const changed = () => { setPlan(null); setDone(false); setError(null); };
  const run = async (confirm: boolean) => {
    setBusy(true); setError(null);
    try {
      const out = await installRuntime(await sourceOf(kind, git, file), { path: git.path.trim(), id: update?.id ?? (git.id.trim() || plan?.id) }, confirm);
      setPlan(out);
      if (confirm) { setDone(true); onInstalled(); }
    } catch (e) { setError(errorMessage(e)); } finally { setBusy(false); }
  };
  const ready = kind === 'git' ? !!git.url.trim() : !!file;
  return {
    kind, git, file, plan, done, busy, error, changed, run, blocked: busy || !ready || stuck(plan, done),
    setKind: (k: Kind) => { setKind(k); changed(); },
    setGit: (g: Partial<typeof git>) => { setGit({ ...git, ...g }); changed(); },
    setFile: (f: File | null) => { setFile(f); changed(); },
  };
}

export default function InstallRuntime({ onInstalled, update, trigger }: { onInstalled: () => void; update?: UpdateOf; trigger?: ReactNode }) {
  const [open, setOpen] = useState(false);
  const s = useInstall(update, onInstalled);
  const reset = (o: boolean) => { setOpen(o); s.changed(); };
  const go = () => { if (s.done) reset(false); else void s.run(s.plan != null); };
  return (
    <Dialog open={open} onOpenChange={reset}>
      {trigger ? <span onClick={() => setOpen(true)}>{trigger}</span> : <Button variant="primary" onClick={() => setOpen(true)}><Download />Install a runtime</Button>}
      <DialogContent title={update ? `Update ${update.id}` : 'Install a runtime'} description="A runtime is the package that runs a world: its parts (the systems agents work against), its tasks and its scoring.">
        <div className="grid max-h-[64vh] gap-3 overflow-auto px-4 py-3 text-sm">
          <p role="note" className="rounded-md border border-warn/50 bg-warn-soft px-3 py-2 text-xs text-fg">{CODE_WARNING}</p>
          <Segmented label="Source" value={s.kind} options={KINDS} onChange={s.setKind} />
          <SourceFields kind={s.kind} git={s.git} onGit={s.setGit} file={s.file} onFile={s.setFile} fixedId={update?.id} />
          {s.done && <p role="status" className="text-xs">Installed: {s.plan?.packages.length} packages registered.</p>}
          {s.plan && <RuntimePlanView plan={s.plan} />}
          {s.error && <p role="alert" className="text-alarm">{s.error}</p>}
          {s.busy && s.plan && <p className="text-xs text-muted">Installing the Python dependencies and running the conformance check: this can take a few minutes.</p>}
        </div>
        <DialogActions onCancel={() => reset(false)} onGo={go} disabled={s.blocked} label={label(s.plan, s.done, s.busy)} />
      </DialogContent>
    </Dialog>
  );
}
