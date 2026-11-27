// The scenario's inputs: its world, the runtime task it plays, and the ticket text (a run plays the task as the runtime defines it).
import { listItems } from '@/api/project';
import { Card } from '@/components/ui/card';
import { useQuery } from '@/hooks/useQuery';
import type { ScenarioBody } from '@/model/project';
import { setIn } from '../edit';
import { ChoiceField, SectionHead, TextField } from '../fields';
import TaskPicker from './TaskPicker';

export default function InputsSection({ scenario, onChange }: { scenario: ScenarioBody; onChange: (s: ScenarioBody) => void }) {
  const worlds = useQuery('items:worlds', () => listItems('worlds'));
  const options = [{ value: '', label: 'Choose a world' }, ...(worlds.data?.items ?? []).map((w) => ({ value: w.id, label: w.label }))];
  return (
    <div className="grid gap-4">
      <SectionHead title="Inputs" about="The world, the task of its runtime the scenario plays, and the ticket written the way you would write it to a person. A run plays the task as the runtime defines it (its customer, its ticket and its checks), starting from the world's data." />
      <Card>
        <div className="grid gap-3 p-[18px]">
          <ChoiceField label="World" value={scenario.world} options={options} onChange={(v) => onChange(setIn(scenario, ['world'], v))} />
          <TaskPicker world={scenario.world} value={scenario.preset ?? ''} onChange={(v) => onChange(setIn(scenario, ['preset'], v))} />
          <TextField label="Ticket" multiline value={scenario.task} onChange={(v) => onChange(setIn(scenario, ['task'], v))} />
        </div>
      </Card>
    </div>
  );
}
