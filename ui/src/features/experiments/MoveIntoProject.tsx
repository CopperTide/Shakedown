// Moving earlier experiments into the project: first what would be written and what can't be
// carried, then Move them.
import { useState } from 'react';
import { type ImportPlan, importProject } from '@/api/project';
import { Button } from '@/components/ui/button';
import { Dialog, DialogActions, DialogContent } from '@/components/ui/dialog';
import { errorMessage } from '@/lib/errors';

export default function MoveIntoProject({ onMoved }: { onMoved: () => void }) {
  const [plan, setPlan] = useState<ImportPlan | null>(null);
  const [error, setError] = useState<string | null>(null);
  const open = async () => { setError(null); try { setPlan(await importProject(false)); } catch (e) { setError(errorMessage(e)); } };
  const move = async () => { try { await importProject(true); setPlan(null); onMoved(); } catch (e) { setError(errorMessage(e)); } };
  return (
    <>
      <Button size="sm" onClick={() => { void open(); }}>Move into the project</Button>
      {error && !plan && <p role="alert" className="text-xs text-alarm">{error}</p>}
      <Dialog open={plan != null} onOpenChange={(o) => { if (!o) setPlan(null); }}>
        <DialogContent title="Move into the project" description="Each becomes files you can edit. Their runs stay; each becomes its experiment's first deployment.">
          <div className="grid max-h-[50vh] gap-3 overflow-auto px-4 py-3 text-sm">
            {plan?.files?.length ? <ul className="grid gap-1">{plan.files.map((f) => <li key={`${f.kind}${f.id}`}>{f.label} <span className="text-faint">· {f.kind}</span></li>)}</ul> : <p className="text-faint">Nothing new to write.</p>}
            {plan?.skipped.length ? <div><p className="font-medium">Not carried</p><ul className="grid gap-1 text-muted">{plan.skipped.map((s) => <li key={s}>{s}</li>)}</ul></div> : null}
            {error && <p role="alert" className="text-alarm">{error}</p>}
          </div>
          <DialogActions onCancel={() => setPlan(null)} onGo={() => { void move(); }} label="Move them" />
        </DialogContent>
      </Dialog>
    </>
  );
}
