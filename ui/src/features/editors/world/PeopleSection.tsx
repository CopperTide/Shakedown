// Simulated people agents can ask, or must ask.
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import type { WorldBody } from '@/model/project';
import { addTo, removeAt } from '../edit';
import { ItemRows, SectionHead, TextField } from '../fields';

export default function PeopleSection({ world, onChange }: { world: WorldBody; onChange: (w: WorldBody) => void }) {
  const list = world.people ?? [];
  const [f, setF] = useState({ id: '', label: '', role: '' });
  const add = () => { onChange({ ...world, people: addTo(list, { id: f.id, label: f.label, ...(f.role ? { role: f.role } : {}) }) }); setF({ id: '', label: '', role: '' }); };
  return (
    <div className="grid gap-4">
      <SectionHead title="People" about="Simulated approvers, planners and operators that agents can ask, or must ask. Optional." />
      <Card>
        <ItemRows items={list} empty="No simulated people yet." onRemove={(i) => onChange({ ...world, people: removeAt(list, i) })}
          render={(p) => <span><span className="font-medium">{p.label}</span> <span className="text-faint">{p.role}</span></span>} />
      </Card>
      <Card title="Add a person">
        <div className="grid gap-3 p-[18px] md:grid-cols-3">
          <TextField label="Person id" mono value={f.id} onChange={(v) => setF({ ...f, id: v })} />
          <TextField label="Person name" value={f.label} onChange={(v) => setF({ ...f, label: v })} placeholder="Planner on call" />
          <TextField label="Role" value={f.role} onChange={(v) => setF({ ...f, role: v })} placeholder="planner" />
        </div>
        <div className="flex justify-end border-t border-line px-[18px] py-2.5"><Button disabled={!f.id || !f.label} onClick={add}>Add person</Button></div>
      </Card>
    </div>
  );
}
