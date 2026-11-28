// Deploying asks first: it starts runs and spends money.
import { Play } from 'lucide-react';
import { useState } from 'react';
import { deployExperiment } from '@/api/project';
import { Button } from '@/components/ui/button';
import { Dialog, DialogActions, DialogContent } from '@/components/ui/dialog';
import { errorMessage } from '@/lib/errors';
import { usd } from '@/lib/format';
import type { ExperimentBody } from '@/model/project';
import { plural } from '@/lib/plural';

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
      <DialogContent title={`Deploy ${plural(runs, 'run')}`} description="Calls real models.">
        <div className="grid gap-2 px-4 py-3 text-sm">
          <p>{plural(body.setups.length, 'setup')} × {plural(body.repeats.per_setup, 'run')} each{body.repeats.paired_seeds === false ? '' : ', paired seeds'}.</p>
          {body.budget?.per_run_usd != null && <p className="text-muted">Each run stops at {usd(body.budget.per_run_usd)}.</p>}
          {error && <p role="alert" className="text-alarm">{error}</p>}
        </div>
        <DialogActions onCancel={() => setOpen(false)} onGo={() => { void go(); }} disabled={busy} label={`Deploy ${plural(runs, 'run')}`} />
      </DialogContent>
    </Dialog>
  );
}
