// Deploying asks first: it starts runs and spends money.
import { Play } from 'lucide-react';
import { useState } from 'react';
import { deployExperiment } from '@/api/project';
import { Button } from '@/components/ui/button';
import { Dialog, DialogActions, DialogContent } from '@/components/ui/dialog';
import { errorMessage } from '@/lib/errors';
import type { ExperimentBody } from '@/model/project';

export default function DeployDialog({ id, body, onDeployed }: { id: string; body: ExperimentBody; onDeployed: () => void }) {
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const runs = body.setups.length * body.repeats.per_setup;
  const go = async () => {
    setBusy(true);
    setError(null);
    try {
      await deployExperiment(id);
      setOpen(false);
      onDeployed();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog open={open} onOpenChange={(o) => { setOpen(o); setError(null); }}>
      <Button variant="primary" onClick={() => setOpen(true)}><Play />Deploy</Button>
      <DialogContent title={`Deploy ${runs} runs`} description="Every run calls a real model and restores the world from its start.">
        <div className="grid gap-2 px-4 py-3 text-sm">
          <p>{body.setups.length} setups × {body.repeats.per_setup} runs each{body.repeats.paired_seeds === false ? '' : ', paired seeds'}.</p>
          {body.budget?.per_run_usd != null && <p className="text-muted">Each run stops at ${body.budget.per_run_usd.toFixed(2)}.</p>}
          {error && <p role="alert" className="text-alarm">{error}</p>}
        </div>
        <DialogActions onCancel={() => setOpen(false)} onGo={() => { void go(); }} disabled={busy} label={`Deploy ${runs} runs`} />
      </DialogContent>
    </Dialog>
  );
}
