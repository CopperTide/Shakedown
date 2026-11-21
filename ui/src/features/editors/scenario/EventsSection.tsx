// Failures and disruptions injected during the run, each at a world time or after agent calls.
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Segmented } from '@/components/ui/segmented';
import type { ScenarioBody, ScenarioEvent } from '@/model/project';
import { addTo, removeAt } from '../edit';
import { ItemRows, SectionHead, TextField } from '../fields';
import { type Timing, setTiming } from './events';

function AddEvent({ onAdd }: { onAdd: (e: ScenarioEvent) => void }) {
  const [name, setName] = useState('');
  const [timing, setTimingKind] = useState<Timing>('time');
  const [when, setWhen] = useState('');
  const [fault, setFault] = useState('');
  const add = () => { onAdd(setTiming({ name, ...(fault ? { fault } : {}) }, timing, when)); setName(''); setWhen(''); setFault(''); };
  return (
    <Card title="Add an event">
      <div className="grid gap-3 p-[18px] md:grid-cols-2">
        <TextField label="Event name" value={name} onChange={setName} placeholder="Supplier slips" />
        <div className="grid gap-1 text-sm"><span className="text-muted">Fires</span>
          <Segmented label="Fires" value={timing} onChange={setTimingKind} options={[{ id: 'time', label: 'At a world time' }, { id: 'calls', label: 'After agent calls' }]} /></div>
        <TextField label={timing === 'time' ? 'World time' : 'Agent calls'} mono value={when} onChange={setWhen} placeholder={timing === 'time' ? '07:30' : '20'} />
        <TextField label="Fault (optional)" mono value={fault} onChange={setFault} placeholder="lock_contention" />
      </div>
      <div className="flex justify-end border-t border-line px-[18px] py-2.5"><Button disabled={!name || !when} onClick={add}>Add event</Button></div>
    </Card>
  );
}

export default function EventsSection({ scenario, onChange }: { scenario: ScenarioBody; onChange: (s: ScenarioBody) => void }) {
  const list = scenario.events ?? [];
  return (
    <div className="grid gap-4">
      <SectionHead title="Events" about="Failures and disruptions, with when each fires. Paired seeds keep the same times across setups." />
      <Card>
        <ItemRows items={list} empty="No events yet." onRemove={(i) => onChange({ ...scenario, events: removeAt(list, i) })}
          render={(e) => <span><span className="font-mono text-faint">{e.at ?? `#${e.after_calls}`}</span> <span className="font-medium">{e.name}</span>{e.fault && <span className="text-faint"> · {e.fault}</span>}</span>} />
      </Card>
      <AddEvent onAdd={(e) => onChange({ ...scenario, events: addTo(list, e) })} />
    </div>
  );
}
