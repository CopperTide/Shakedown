// What must be true at the end: goals, and safety rules that hold no matter how the goal is reached.
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Toggle } from '@/components/ui/toggle';
import type { Check, ScenarioBody } from '@/model/project';
import { addTo, removeAt } from '../edit';
import { ChoiceField, ItemRows, SectionHead, TextField } from '../fields';

function Group({ title, hint, checks, onRemove }: { title: string; hint: string; checks: { check: Check; i: number }[]; onRemove: (i: number) => void }) {
  return (
    <section aria-label={title}>
      <Card title={title} hint={hint}>
        <ItemRows items={checks} empty="None yet." onRemove={(j) => onRemove(checks[j].i)}
          render={({ check }) => <span>{check.text}{check.ends_run && <span className="text-faint"> · ends the run</span>}</span>} />
      </Card>
    </section>
  );
}

export default function ChecksSection({ scenario, onChange }: { scenario: ScenarioBody; onChange: (s: ScenarioBody) => void }) {
  const list = scenario.checks ?? [];
  const [f, setF] = useState<Check>({ text: '', kind: 'goal' });
  const indexed = list.map((check, i) => ({ check, i }));
  const remove = (i: number) => onChange({ ...scenario, checks: removeAt(list, i) });
  const add = () => { onChange({ ...scenario, checks: addTo(list, { ...f, ...(f.ends_run ? {} : { ends_run: undefined }) }) }); setF({ text: '', kind: 'goal' }); };
  return (
    <div className="grid gap-4">
      <SectionHead title="Checks" about="What must be true at the end, in plain language. Saved with the scenario. Runs use them once worlds run from their own files." />
      <Group title="Goals" hint="What a good outcome looks like." checks={indexed.filter((c) => c.check.kind === 'goal')} onRemove={remove} />
      <Group title="Safety rules" hint="Must hold no matter how the goal is reached." checks={indexed.filter((c) => c.check.kind === 'safety')} onRemove={remove} />
      <Card title="Add a check">
        <div className="grid gap-3 p-[18px] md:grid-cols-2">
          <TextField label="Check" value={f.text} onChange={(v) => setF({ ...f, text: v })} placeholder="SO-1001 ships by Wed 14:00" />
          <ChoiceField label="Kind" value={f.kind} options={[{ value: 'goal', label: 'Goal' }, { value: 'safety', label: 'Safety rule' }]} onChange={(v) => setF({ ...f, kind: v })} />
          <Toggle label="Ends the run when true" checked={!!f.ends_run} onChange={(v) => setF({ ...f, ends_run: v })} />
        </div>
        <div className="flex justify-end border-t border-line px-[18px] py-2.5"><Button disabled={!f.text} onClick={add}>Add check</Button></div>
      </Card>
    </div>
  );
}
