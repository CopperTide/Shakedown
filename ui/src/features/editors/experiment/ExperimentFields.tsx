// The choices an experiment is made of: where it runs, what happens, who is compared, how often,
// and what a run may spend.
import { useState } from 'react';
import { Card } from '@/components/ui/card';
import { Checkbox, Input } from '@/components/ui/input';
import type { ExperimentBody } from '@/model/project';
import { setIn } from '../edit';
import { ChoiceField } from '../fields';
import { capOf, capProblem, capText, sameCap, withRunCap } from './budget';
import { type Choices, scenariosOf } from './choices';

function Setups({ body, choices, onChange }: { body: ExperimentBody; choices: Choices; onChange: (b: ExperimentBody) => void }) {
  const toggle = (id: string, on: boolean) => onChange({ ...body, setups: on ? [...body.setups, id] : body.setups.filter((s) => s !== id) });
  return (
    <fieldset className="grid gap-2 text-sm">
      <legend className="mb-1 text-muted">Setups to compare</legend>
      {choices.setups.map((s) => (
        <label key={s.id} className="flex items-center gap-2"><Checkbox aria-label={s.label} checked={body.setups.includes(s.id)} onChange={(e) => toggle(s.id, e.target.checked)} />{s.label}</label>
      ))}
      {choices.setups.length === 0 && <p className="text-faint">No agent setups yet.</p>}
    </fieldset>
  );
}

/** The per-run spend cap in dollars, as typed; the body holds what it reads as (see `capProblem`). */
function RunCap({ body, onChange }: { body: ExperimentBody; onChange: (b: ExperimentBody) => void }) {
  const cap = body.budget?.per_run_usd ?? undefined; // a null in the file is no cap too
  const [text, setText] = useState(capText(cap));
  // A cap changed from outside (Discard) shows instead of what was typed.
  if (!sameCap(capOf(text), cap)) setText(capText(cap));
  const problem = capProblem(body);
  return (
    <label className="grid gap-1 text-sm"><span className="text-muted">Spend cap per run (USD)</span>
      <Input aria-label="Spend cap per run (USD)" inputMode="decimal" placeholder="No cap" value={text} aria-invalid={!!problem}
        onChange={(e) => { setText(e.target.value); onChange(withRunCap(body, capOf(e.target.value))); }} />
      <span className={problem ? 'text-xs text-alarm' : 'text-xs text-faint'}>{problem ?? 'A run stops once its model spend passes this. Empty: no cap.'}</span>
    </label>
  );
}

export default function ExperimentFields({ body, choices, onChange }: { body: ExperimentBody; choices: Choices; onChange: (b: ExperimentBody) => void }) {
  const worlds = [{ value: '', label: 'Choose a world' }, ...choices.worlds.map((w) => ({ value: w.id, label: w.label }))];
  const scenarios = [{ value: '', label: 'Choose a scenario' }, ...scenariosOf(choices.scenarios, body.world).map((s) => ({ value: s.id, label: s.label }))];
  return (
    <Card>
      <div className="grid gap-4 p-[18px] md:grid-cols-2">
        <ChoiceField label="World" value={body.world} options={worlds} onChange={(v) => onChange({ ...body, world: v, scenario: '' })} />
        <ChoiceField label="Scenario" value={body.scenario} options={scenarios} onChange={(v) => onChange({ ...body, scenario: v })} />
        <Setups body={body} choices={choices} onChange={onChange} />
        <label className="grid gap-1 text-sm"><span className="text-muted">Runs per setup</span>
          <Input aria-label="Runs per setup" type="number" min={1} value={body.repeats.per_setup}
            onChange={(e) => onChange(setIn(body, ['repeats', 'per_setup'], Math.max(1, Number(e.target.value) || 1)))} /></label>
        <RunCap body={body} onChange={onChange} />
      </div>
    </Card>
  );
}
