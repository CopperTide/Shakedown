// Observability and on-call: the tools runs report to, the people who answer pages, and what
// counts as needing a human.
import { useState } from 'react';
import { Link } from 'react-router-dom';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import type { Observability, WorldBody } from '@/model/project';
import { addTo, removeAt } from '../edit';
import { ChoiceField, ItemRows, SectionHead, TextField } from '../fields';
import ObsTools from './ObsTools';
import { MOMENTS, behaviorOf, setRule, withObservability } from './observability';

const BEHAVIORS = [{ value: 'answers_after', label: 'Answers after some minutes' }, { value: 'approves', label: 'Approves everything' }, { value: 'refuses', label: 'Refuses everything' }, { value: 'never_answers', label: 'Never answers' }];
const THEN = [{ value: '', label: 'Not counted' }, { value: 'continue', label: 'Continue and report' }, { value: 'pause', label: 'Pause the run' }, { value: 'fail', label: 'Fail the run' }];

function Operators({ obs, onChange }: { obs: Observability; onChange: (o: Observability) => void }) {
  const [f, setF] = useState({ id: '', label: '', behavior: 'answers_after', minutes: '15' });
  const list = obs.operators ?? [];
  const add = () => { onChange({ ...obs, operators: addTo(list, { id: f.id, label: f.label, behavior: behaviorOf(f.behavior, f.minutes) }) }); setF({ ...f, id: '', label: '' }); };
  return (
    <Card title="Simulated operators" hint="On-call people who receive pages and approval requests.">
      <ItemRows items={list} empty="No operators yet." onRemove={(i) => onChange({ ...obs, operators: removeAt(list, i) })}
        render={(o) => <span><span className="font-medium">{o.label}</span> <span className="text-faint">· {typeof o.behavior === 'string' ? o.behavior.replace('_', ' ') : `answers after ${o.behavior.answers_after.minutes} min`}</span></span>} />
      <div className="grid gap-3 border-t border-line p-[18px] md:grid-cols-2">
        <TextField label="Operator id" mono value={f.id} onChange={(v) => setF({ ...f, id: v })} />
        <TextField label="Operator name" value={f.label} onChange={(v) => setF({ ...f, label: v })} placeholder="Planner on call" />
        <ChoiceField label="Behavior" value={f.behavior} options={BEHAVIORS} onChange={(v) => setF({ ...f, behavior: v })} />
        {f.behavior === 'answers_after' && <TextField label="Minutes" mono value={f.minutes} onChange={(v) => setF({ ...f, minutes: v })} />}
      </div>
      <div className="flex justify-end border-t border-line px-[18px] py-2.5"><Button disabled={!f.id || !f.label} onClick={add}>Add operator</Button></div>
    </Card>
  );
}

function HumanRules({ obs, onChange }: { obs: Observability; onChange: (o: Observability) => void }) {
  const value = (m: string) => obs.human?.find((r) => r.moment === m)?.then ?? '';
  return (
    <Card title="Needs a human" hint={'What counts as needing a human.\nSaved with the world. Runs report these moments once worlds run from their own files.'}>
      <div className="grid gap-3 p-[18px] md:grid-cols-2">
        {MOMENTS.map((m) => <ChoiceField key={m.moment} label={m.label} value={value(m.moment)} options={THEN} onChange={(v) => onChange({ ...obs, human: setRule(obs.human, m.moment, v) })} />)}
      </div>
    </Card>
  );
}

export default function ObservabilitySection({ world, onChange }: { world: WorldBody; onChange: (w: WorldBody) => void }) {
  const obs = world.observability ?? {};
  const set = (o: Observability) => onChange(withObservability(world, o));
  return (
    <div className="grid gap-4">
      <SectionHead title="Observability & on-call" about="Declare monitoring tools and simulated operators for this world. These declarations do not configure telemetry export." />
      <Card title="Telemetry export" hint="Send completed runs to Langfuse, Arize Phoenix, or an OTLP collector. Destinations, connection tests and export history are in project settings.">
        <div className="px-[18px] py-3"><Button asChild><Link to="/settings/observability">Open telemetry destinations</Link></Button></div>
      </Card>
      <ObsTools obs={obs} onChange={set} />
      <Operators obs={obs} onChange={set} />
      <HumanRules obs={obs} onChange={set} />
    </div>
  );
}
