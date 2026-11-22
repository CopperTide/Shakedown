// Rules that must hold during the run, saved with the scenario.
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import type { Gate, ScenarioBody } from '@/model/project';
import { addTo, removeAt } from '../edit';
import { ChoiceField, ItemRows, SectionHead, TextField } from '../fields';

const BREACH = [{ value: 'block', label: 'Blocks the call' }, { value: 'flag', label: 'Flags it for review' }] as const;

export default function GatesSection({ scenario, onChange }: { scenario: ScenarioBody; onChange: (s: ScenarioBody) => void }) {
  const list = scenario.gates ?? [];
  const [f, setF] = useState<Gate>({ rule: '', scope: '', on_breach: 'block' });
  const add = () => { onChange({ ...scenario, gates: addTo(list, f) }); setF({ rule: '', scope: '', on_breach: 'block' }); };
  return (
    <div className="grid gap-4">
      <SectionHead title="Gates" about="Rules that must hold during the run. Saved with the scenario. Runs check them once worlds run from their own files." />
      <Card>
        <ItemRows items={list} empty="No gates yet." onRemove={(i) => onChange({ ...scenario, gates: removeAt(list, i) })}
          render={(g) => <span><span className="font-medium">{g.rule}</span> <span className="text-faint">· {g.scope} · {g.on_breach === 'block' ? 'blocks' : 'flags'}</span></span>} />
      </Card>
      <Card title="Add a gate">
        <div className="grid gap-3 p-[18px] md:grid-cols-3">
          <TextField label="Rule" value={f.rule} onChange={(v) => setF({ ...f, rule: v })} placeholder="PO over $150 needs approval" />
          <TextField label="Applies to tool" mono value={f.scope} onChange={(v) => setF({ ...f, scope: v })} placeholder="erp.create_purchase_order" />
          <ChoiceField label="On a breach" value={f.on_breach} options={[...BREACH]} onChange={(v) => setF({ ...f, on_breach: v })} />
        </div>
        <div className="flex justify-end border-t border-line px-[18px] py-2.5"><Button disabled={!f.rule || !f.scope} onClick={add}>Add gate</Button></div>
      </Card>
    </div>
  );
}
