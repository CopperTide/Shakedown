// Creating an item: a name, an id (suggested from the name), checked before anything is written.
// A new world starts empty; its runtime can be picked here from those installed.
import { Plus } from 'lucide-react';
import { useState } from 'react';
import { putItem } from '@/api/project';
import { Button } from '@/components/ui/button';
import { Dialog, DialogActions, DialogContent } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { errorMessage } from '@/lib/errors';
import { CREATED, track } from '@/lib/productAnalytics';
import type { ItemKind } from '@/model/project';
import { ID_RULE, idFor, isValidId } from './idRule';
import { KINDS } from './kinds';
import { TEMPLATES } from './templates';
import { ChoiceField } from './fields';
import { runtimeOptions, useRuntimes } from './world/RuntimeSection';

/** A new world's runtime: one of the installed ones, or none yet. */
function RuntimeChoice({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const installed = useRuntimes();
  return <ChoiceField label="Runtime" value={value} options={runtimeOptions(installed.data)} onChange={onChange} />;
}

/** Why `id` can't be used, or null when it can. */
export function idProblem(kind: ItemKind, id: string, taken: string[]): string | null {
  if (!isValidId(id)) return ID_RULE;
  if (taken.includes(id)) return `A ${KINDS[kind].one} named ${id} already exists.`;
  return null;
}

export default function NewItemDialog({ kind, taken, onCreated }: { kind: ItemKind; taken: string[]; onCreated: (id: string) => void }) {
  const [open, setOpen] = useState(false);
  const [label, setLabel] = useState('');
  const [id, setId] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [runtime, setRuntime] = useState('');
  const create = async () => {
    const problem = idProblem(kind, id, taken);
    if (problem) { setError(problem); return; }
    try {
      const body = TEMPLATES[kind](label);
      await putItem(kind, id, kind === 'worlds' && runtime ? { ...body, package: runtime } : body, true);
      track(CREATED[kind]);
      setOpen(false);
      onCreated(id);
    } catch (e) {
      setError(errorMessage(e));
    }
  };
  return (
    <Dialog open={open} onOpenChange={(o) => { setOpen(o); setError(null); }}>
      <Button variant="primary" onClick={() => setOpen(true)}><Plus />New {KINDS[kind].one}</Button>
      <DialogContent title={`New ${KINDS[kind].one}`}>
        <div className="grid gap-3 px-4 py-3 text-sm">
          <label className="grid gap-1"><span>Name</span><Input aria-label="Name" value={label} onChange={(e) => { setLabel(e.target.value); setId(idFor(e.target.value)); }} placeholder={KINDS[kind].example} /></label>
          <label className="grid gap-1"><span>Id</span><Input aria-label="Id" value={id} onChange={(e) => setId(e.target.value)} className="font-mono" /></label>
          {kind === 'worlds' && <RuntimeChoice value={runtime} onChange={setRuntime} />}
          {error && <p role="alert" className="text-alarm">{error}</p>}
        </div>
        <DialogActions onCancel={() => setOpen(false)} onGo={() => { void create(); }} disabled={!label.trim()} label="Create" />
      </DialogContent>
    </Dialog>
  );
}
