// Deterministic automations that already run in production, with no agent involved.
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import type { WorldBody } from '@/model/project';
import { addTo, removeAt } from '../edit';
import { ItemRows, SectionHead, TextField } from '../fields';

/** The steps written one per line, without blank lines. */
export const stepsOf = (text: string) => text.split('\n').map((s) => s.trim()).filter(Boolean);

export default function FlowsSection({ world, onChange }: { world: WorldBody; onChange: (w: WorldBody) => void }) {
  const list = world.flows ?? [];
  const [f, setF] = useState({ id: '', trigger: '', steps: '' });
  const add = () => { onChange({ ...world, flows: addTo(list, { id: f.id, trigger: f.trigger, steps: stepsOf(f.steps) }) }); setF({ id: '', trigger: '', steps: '' }); };
  return (
    <div className="grid gap-4">
      <SectionHead title="Software flows" about="Automations that already run in production with no agent involved. Saved with the world. Runs use them once worlds run from their own files." />
      <Card>
        <ItemRows items={list} empty="No software flows yet." onRemove={(i) => onChange({ ...world, flows: removeAt(list, i) })}
          render={(fl) => <span><span className="font-medium">{fl.id}</span> <span className="text-faint">when {fl.trigger} · {fl.steps.length} steps</span></span>} />
      </Card>
      <Card title="Add a flow">
        <div className="grid gap-3 p-[18px] md:grid-cols-2">
          <TextField label="Flow id" mono value={f.id} onChange={(v) => setF({ ...f, id: v })} />
          <TextField label="Trigger" mono placeholder="crm.order.created" value={f.trigger} onChange={(v) => setF({ ...f, trigger: v })} />
          <TextField label="Steps" mono multiline placeholder="One connector tool per line" value={f.steps} onChange={(v) => setF({ ...f, steps: v })} />
        </div>
        <div className="flex justify-end border-t border-line px-[18px] py-2.5"><Button disabled={!f.id || !f.trigger} onClick={add}>Add flow</Button></div>
      </Card>
    </div>
  );
}
