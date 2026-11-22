// The scenario's inputs: its world, the package task it plays, and the ticket text (runs use the preset task's ticket for now).
import { listItems } from '@/api/project';
import { Card } from '@/components/ui/card';
import { useQuery } from '@/hooks/useQuery';
import type { ScenarioBody } from '@/model/project';
import { setIn } from '../edit';
import { ChoiceField, SectionHead, TextField } from '../fields';

export default function InputsSection({ scenario, onChange }: { scenario: ScenarioBody; onChange: (s: ScenarioBody) => void }) {
  const worlds = useQuery('items:worlds', () => listItems('worlds'));
  const options = [{ value: '', label: 'Choose a world' }, ...(worlds.data?.items ?? []).map((w) => ({ value: w.id, label: w.label }))];
  return (
    <div className="grid gap-4">
      <SectionHead title="Inputs" about="The ticket, written the way you would write it to a person. Saved with the scenario. Runs use the ticket of the preset task until worlds run from their own files." />
      <Card>
        <div className="grid gap-3 p-[18px]">
          <ChoiceField label="World" value={scenario.world} options={options} onChange={(v) => onChange(setIn(scenario, ['world'], v))} />
          <TextField label="Task of the world's runtime it plays" mono placeholder="t1" value={scenario.preset ?? ''} onChange={(v) => onChange(setIn(scenario, ['preset'], v || undefined))} />
          <TextField label="Ticket" multiline value={scenario.task} onChange={(v) => onChange(setIn(scenario, ['task'], v))} />
        </div>
      </Card>
    </div>
  );
}
