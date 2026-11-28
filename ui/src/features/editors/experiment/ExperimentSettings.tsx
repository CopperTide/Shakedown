// The Settings tab of a file experiment: its choices, editable and saved to its file.
import { useState } from 'react';
import { putItem } from '@/api/project';
import { Button } from '@/components/ui/button';
import { ErrorState } from '@/components/ui/state';
import { useQuery } from '@/hooks/useQuery';
import { errorMessage } from '@/lib/errors';
import type { ExperimentBody } from '@/model/project';
import { capProblem } from './budget';
import { loadChoices } from './choices';
import ExperimentFields from './ExperimentFields';

export default function ExperimentSettings({ id, body, revision, onSaved }: { id: string; body: ExperimentBody; revision?: number; onSaved: () => void }) {
  const choices = useQuery('experiment-choices', loadChoices);
  const [draft, setDraft] = useState(body);
  const [error, setError] = useState<string | null>(null);
  const dirty = JSON.stringify(draft) !== JSON.stringify(body);
  const save = async () => {
    try { await putItem('experiments', id, draft, true, revision); setError(null); onSaved(); } catch (e) { setError(errorMessage(e)); }
  };
  if (choices.error) return <ErrorState error={choices.error} onRetry={choices.reload} />;
  if (!choices.data) return null;
  return (
    <div className="grid gap-3">
      <ExperimentFields body={draft} choices={choices.data} onChange={setDraft} />
      {error && <p role="alert" className="text-sm text-alarm">{error}</p>}
      <div className="flex justify-end gap-2">
        <Button disabled={!dirty} onClick={() => setDraft(body)}>Discard</Button>
        <Button variant="primary" disabled={!dirty || !!capProblem(draft)} onClick={() => { void save(); }}>Save</Button>
      </div>
    </div>
  );
}
