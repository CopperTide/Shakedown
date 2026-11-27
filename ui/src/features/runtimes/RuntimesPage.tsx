// Runtimes: the packages that run worlds. Each install with where it came from, its packages
// (kind, version, conformance check, the worlds they run), Check again, Update and Remove; then
// the packages installed from folders. Install a runtime opens the install dialog.
import { useState } from 'react';
import { Link } from 'react-router-dom';
import { type RuntimeInstall, type RuntimePackage, checkRuntime, removeRuntime, runtimes } from '@/api/runtimes';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { PageBody, PageHeader } from '@/components/ui/page';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/state';
import { useQuery } from '@/hooks/useQuery';
import { errorMessage } from '@/lib/errors';
import CheckResult from './CheckResult';
import InstallRuntime, { CODE_WARNING } from './InstallRuntime';

/** Where an install came from, on one line. */
export function sourceLine(i: RuntimeInstall): string {
  const folder = i.path ? ` · folder ${i.path}` : '';
  if (i.source === 'archive') return `Archive ${i.url} · sha256 ${i.commit.slice(0, 12)}${folder}`;
  return `${i.url} · ${i.ref} at ${i.commit.slice(0, 12)}${folder}`;
}

function UsedBy({ worlds }: { worlds: string[] }) {
  if (!worlds.length) return <span className="text-faint">no world</span>;
  return <>{worlds.map((w, n) => <span key={w}>{n > 0 && ', '}<Link className="text-primary" to={`/worlds/${w}`}>{w}</Link></span>)}</>;
}

function PackageRows({ packages }: { packages: RuntimePackage[] }) {
  return (
    <table className="w-full text-sm">
      <thead><tr className="text-left text-2xs text-muted"><th className="px-[18px] py-1.5 font-medium">Package</th><th className="pr-4 font-medium">Kind</th><th className="pr-4 font-medium">Version</th><th className="pr-4 font-medium">Worlds</th><th className="pr-[18px] font-medium">Check</th></tr></thead>
      <tbody className="divide-y divide-line">
        {packages.map((p) => (
          <tr key={p.name} className="align-top">
            <td className="px-[18px] py-2 font-mono text-xs">{p.name}{p.registered === false && <span className="ml-1 text-warn">(not registered)</span>}</td>
            <td className="py-2 pr-4 text-muted">{p.kind}</td>
            <td className="py-2 pr-4 font-mono text-xs text-muted">{p.version ?? '—'}</td>
            <td className="py-2 pr-4"><UsedBy worlds={p.used_by} /></td>
            <td className="py-2 pr-[18px]">{p.check ? <CheckResult check={p.check} named={false} /> : <span className="text-xs text-faint">not checked</span>}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function Install({ install, onChange, hosted }: { install: RuntimeInstall; onChange: () => void; hosted: boolean }) {
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const act = (what: string, run: () => Promise<unknown>) => async () => {
    setBusy(what); setError(null);
    try { await run(); onChange(); } catch (e) { setError(errorMessage(e)); } finally { setBusy(null); }
  };
  const update = install.source === 'git' ? { id: install.id, url: install.url, ref: install.ref, path: install.path } : { id: install.id, path: install.path };
  const actions = install.runtime && !hosted
    ? (
      <div className="flex gap-2">
        <Button size="sm" disabled={!!busy} onClick={act('check', () => checkRuntime(install.id))}>{busy === 'check' ? 'Checking…' : 'Check again'}</Button>
        <InstallRuntime update={update} onInstalled={onChange} trigger={<Button size="sm" disabled={!!busy}>Update</Button>} />
        <Button size="sm" variant="danger" disabled={!!busy} onClick={act('remove', () => removeRuntime(install.id))}>{busy === 'remove' ? 'Removing…' : 'Remove'}</Button>
      </div>
    )
    : <span className="text-xs text-faint">{install.runtime ? '' : 'From Import from git, with its project files'}</span>;
  return (
    <Card title={install.id} description={<><span className="font-mono">{sourceLine(install)}</span> · installed {new Date(install.installed_at).toLocaleString()}</>} actions={actions}>
      {error && <p role="alert" className="border-b border-line px-[18px] py-2 text-sm text-alarm">{error}</p>}
      <PackageRows packages={install.packages} />
    </Card>
  );
}

/** What installing a runtime means here: code that runs on this machine, or (hosted) not yet. */
function Banner({ hosted }: { hosted: boolean }) {
  if (hosted) return <p role="note" className="rounded-md border border-line bg-raised px-3 py-2 text-sm">Hosted Shakedown doesn't install runtimes yet: a runtime is code, and hosted runs code only in a run's sandbox, which can't install one yet.</p>;
  return <p role="note" className="rounded-md border border-warn/50 bg-warn-soft px-3 py-2 text-sm">{CODE_WARNING}</p>;
}

export default function RuntimesPage() {
  const q = useQuery('runtimes', runtimes);
  const data = q.data;
  const install = data && !data.hosted ? <InstallRuntime onInstalled={q.reload} /> : null;
  return (
    <>
      <PageHeader title="Runtimes" meta="The packages that run worlds: their parts, tasks and scoring. A world names its runtime in its Runtime section." actions={install} width="wide" />
      <PageBody width="wide" className="grid gap-4">
        <Banner hosted={!!data?.hosted} />
        {q.error && <ErrorState error={q.error} onRetry={q.reload} />}
        {!data && !q.error && <LoadingState label="Loading the runtimes" />}
        {data && data.installs.length === 0 && <Card><EmptyState title="No runtime installed yet." hint="Install one from a git URL or an archive to build worlds on it." action={install} /></Card>}
        {data?.installs.map((i) => <Install key={i.id} install={i} onChange={q.reload} hosted={data.hosted} />)}
        {data && data.builtin.length > 0 && (
          <Card title="Installed with Shakedown" description="Packages in this Shakedown's package folders (SHAKEDOWN_PACKAGES).">
            <PackageRows packages={data.builtin} />
          </Card>
        )}
      </PageBody>
    </>
  );
}
