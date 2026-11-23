// A new experiment: name it, pick its world, scenario and setups, say how often, and create it as a
// file in the project. Nothing runs until it is deployed.
import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { putItem } from '@/api/project';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { PageBody, PageHeader } from '@/components/ui/page';
import { ErrorState } from '@/components/ui/state';
import { useQuery } from '@/hooks/useQuery';
import { errorMessage } from '@/lib/errors';
import { track } from '@/lib/productAnalytics';
import type { ExperimentBody } from '@/model/project';
import { idFor } from '../idRule';
import { loadChoices } from './choices';
import ExperimentFields from './ExperimentFields';

const EMPTY: ExperimentBody = { format: 'shakedown-experiment/1', label: '', world: '', scenario: '', setups: [], repeats: { per_setup: 1, paired_seeds: true } };

export default function NewExperimentPage() {
  const choices = useQuery('experiment-choices', loadChoices);
  const [body, setBody] = useState(EMPTY);
  const [error, setError] = useState<string | null>(null);
  const navigate = useNavigate();
  const ready = body.label.trim() && body.world && body.scenario && body.setups.length > 0;
  const create = async () => {
    const id = idFor(body.label);
    try {
      await putItem('experiments', id, body, true);
      track('experiment created');
      navigate(`/experiments/file/${id}`);
    } catch (e) { setError(errorMessage(e)); }
  };
  return (
    <>
      <PageHeader title="New experiment" meta="Pick a world, a scenario in it and the setups to compare. Nothing runs until you deploy." width="default" />
      <PageBody width="default">
        {choices.error && <ErrorState error={choices.error} onRetry={choices.reload} />}
        <label className="grid gap-1 text-sm"><span className="text-muted">Name</span>
          <Input aria-label="Name" value={body.label} onChange={(e) => setBody({ ...body, label: e.target.value })} placeholder="Supplier slip: solo vs team" /></label>
        {choices.data && <ExperimentFields body={body} choices={choices.data} onChange={setBody} />}
        {error && <p role="alert" className="text-sm text-alarm">{error}</p>}
        <div className="flex justify-end"><Button variant="primary" disabled={!ready} onClick={() => { void create(); }}>Create experiment</Button></div>
      </PageBody>
    </>
  );
}
