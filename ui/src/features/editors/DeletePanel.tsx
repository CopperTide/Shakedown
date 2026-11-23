// What a delete shows before anything goes, in the delete dialog and in the assistant's delete
// card: what uses the item, what stays, and the choice of deleting it alone or with what uses it.
import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { ApiError } from '@/api/client';
import { deleteItem, dependentsOf, restoreDeleted } from '@/api/project';
import { Button } from '@/components/ui/button';
import { LoadingState } from '@/components/ui/state';
import { projectChanged } from '@/hooks/useProjectItems';
import { errorMessage } from '@/lib/errors';
import type { Dependent, ItemKind } from '@/model/project';
import { KEPT_AS, cascadeLabel, fatesOf } from './deletion';
import { KINDS } from './kinds';

function UsedBy({ deps }: { deps: Dependent[] }) {
  const sections: [string, Dependent[]][] = [['Scenarios', deps.filter((d) => d.kind === 'scenario')], ['Experiments', deps.filter((d) => d.kind === 'experiment')]];
  return (
    <div className="grid gap-3">
      {sections.filter(([, list]) => list.length > 0).map(([title, list]) => (
        <div key={title}>
          <h3 className="text-2xs font-medium text-faint">{title} using it</h3>
          <ul className="mt-1 divide-y divide-line rounded-md border border-line">
            {list.map((d) => (
              <li key={d.id} className="flex items-center gap-2 px-2.5 py-1.5">
                <span className="font-medium">{d.label}</span>
                <span className="font-mono text-2xs text-faint">{d.id}</span>
                <span className="ml-auto text-2xs text-faint">{fatesOf(d).map((f) => KEPT_AS[f]).filter(Boolean).join(', ')}</span>
              </li>
            ))}
          </ul>
        </div>
      ))}
    </div>
  );
}

function Notes({ kind, id, deps, kept }: { kind: ItemKind; id: string; deps: Dependent[]; kept: string | null }) {
  const dropsSetup = deps.some((d) => fatesOf(d).includes('setup_dropped'));
  const losesRuns = kind === 'experiments' || deps.some((d) => d.kind === 'experiment' && d.fate === 'deleted');
  return (
    <ul className="grid list-disc gap-1 pl-4 text-xs text-muted">
      {dropsSetup && <li>Experiments comparing other setups keep their other setups and only lose this one. An experiment left with no setups is deleted.</li>}
      {kind === 'worlds' && <li>The world&apos;s data folder {kept} stays. Only its world.yaml is deleted.</li>}
      {kind === 'setups' && kept && <li>The setup&apos;s folder {kept} stays; only {id}.yaml is deleted.</li>}
      <li>{losesRuns ? 'Past runs of a deleted experiment stay, listed under Earlier experiments on the Experiments page.' : 'Past runs and results stay viewable.'}</li>
      <li>Deleted files go to the project&apos;s trash, .shakedown/trash/, and Undo puts them back.</li>
    </ul>
  );
}

/** Puts back the delete whose files went to `trash`, and says so. */
function undo(trash: string, label: string) {
  restoreDeleted(trash).then(() => { projectChanged(); toast(`Restored ${label}`); }, (e: unknown) => toast.error(errorMessage(e)));
}

/** What uses the item (asked when shown), and deleting it; a refusal shows what uses it now. */
function useDeletion(kind: ItemKind, id: string, label: string, onDeleted: () => void) {
  const [deps, setDeps] = useState<Dependent[] | null>(null);
  const [kept, setKept] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    dependentsOf(kind, id).then((r) => { setDeps(r.dependents); setKept(r.kept); }, (e: unknown) => setError(errorMessage(e)));
  }, [kind, id]);
  const refused = (e: unknown) => {
    const fresh = e instanceof ApiError && e.status === 409 ? (e.body as { dependents?: Dependent[] } | undefined)?.dependents : undefined;
    if (fresh) setDeps(fresh);
    setError(fresh ? 'What uses it changed since this opened. Check the list, then choose again.' : errorMessage(e));
  };
  const remove = (cascade: boolean) => {
    setBusy(true);
    deleteItem(kind, id, cascade, cascade ? deps ?? [] : undefined)
      .then((done) => {
        projectChanged();
        toast(`Deleted ${label}`, { action: { label: 'Undo', onClick: () => undo(done.trash, label) } });
        onDeleted();
      }, refused)
      .finally(() => setBusy(false));
  };
  return { deps, kept, error, busy, remove };
}

function Choices({ deps, busy, onCancel, onDelete }: { deps: Dependent[] | null; busy: boolean; onCancel: () => void; onDelete: (cascade: boolean) => void }) {
  const used = deps != null && deps.length > 0;
  return (
    <div className="flex flex-wrap justify-end gap-2 border-t border-line px-4 py-2.5">
      <Button onClick={onCancel}>Cancel</Button>
      {used
        ? <>
          <Button variant="danger" disabled>Delete only if unused</Button>
          <Button variant="danger" disabled={busy} onClick={() => onDelete(true)}>{cascadeLabel(deps)}</Button>
        </>
        : <Button variant="danger" disabled={deps == null || busy} onClick={() => onDelete(false)}>Delete</Button>}
    </div>
  );
}

export default function DeletePanel({ kind, id, label, onDeleted, onCancel }: {
  kind: ItemKind;
  id: string;
  label: string;
  onDeleted: () => void;
  onCancel: () => void;
}) {
  const { deps, kept, error, busy, remove } = useDeletion(kind, id, label, onDeleted);
  const one = KINDS[kind].one;
  return (
    <>
      <div className="grid gap-3 overflow-y-auto px-4 py-3 text-sm">
        {deps == null && !error && <LoadingState label="Checking what uses it" rows={2} />}
        {deps?.length === 0 && <p>Nothing uses this {one}.</p>}
        {deps && deps.length > 0 && <UsedBy deps={deps} />}
        {deps && <Notes kind={kind} id={id} deps={deps} kept={kept} />}
        {error && <p role="alert" className="text-alarm">{error}</p>}
      </div>
      <Choices deps={deps} busy={busy} onCancel={onCancel} onDelete={remove} />
    </>
  );
}
