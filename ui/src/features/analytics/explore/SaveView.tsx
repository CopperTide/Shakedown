// "Save view": name what is on screen (a builder query or an SQL query) and keep it in the saved
// views, for anyone who can see this project.
import { Bookmark } from 'lucide-react';
import { useState } from 'react';
import { type NewView, saveView } from '@/api/explore';
import { Button } from '@/components/ui/button';
import { Dialog, DialogActions, DialogContent } from '@/components/ui/dialog';
import { Field, Input } from '@/components/ui/input';
import { errorMessage } from '@/lib/errors';

export default function SaveView({ view, suggest, onSaved }: { view: () => Omit<NewView, 'name'>; suggest: string; onSaved?: () => void }) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState('');
  const [state, setState] = useState<{ busy: boolean; error: string | null; done: string | null }>({ busy: false, error: null, done: null });
  const go = async () => {
    setState({ busy: true, error: null, done: null });
    try {
      const saved = await saveView({ ...view(), name: name.trim() } as NewView);
      setState({ busy: false, error: null, done: saved.name });
      setOpen(false);
      onSaved?.();
    } catch (e) {
      setState({ busy: false, error: errorMessage(e), done: null });
    }
  };
  return (
    <>
      <Button size="sm" onClick={() => { setName(suggest); setState({ busy: false, error: null, done: null }); setOpen(true); }}><Bookmark />Save view</Button>
      {state.done && <span role="status" className="text-xs text-muted">Saved as {state.done}</span>}
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent title="Save this view" description="It appears under Saved views, ready to open again.">
          <div className="p-4">
            <Field label="Name">
              <Input autoFocus maxLength={120} value={name} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => { if (e.key === 'Enter' && name.trim()) void go(); }} className="h-8" />
            </Field>
            {state.error && <p role="alert" className="mt-2 text-xs text-alarm">{state.error}</p>}
          </div>
          <DialogActions label={state.busy ? 'Saving…' : 'Save'} onCancel={() => setOpen(false)} onGo={() => void go()} disabled={state.busy || !name.trim()} />
        </DialogContent>
      </Dialog>
    </>
  );
}
