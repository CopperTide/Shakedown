// Monitoring and alerting tools: real (they receive the sandbox's telemetry, tagged by run) or
// built-in stand-ins seeded from an export.
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import type { Observability } from '@/model/project';
import { addTo, removeAt } from '../edit';
import { ChoiceField, ItemRows, TextField } from '../fields';

type Tool = NonNullable<Observability['tools']>[number];

export default function ObsTools({ obs, onChange }: { obs: Observability; onChange: (o: Observability) => void }) {
  const [f, setF] = useState({ id: '', product: '', mode: 'stand-in', seeded: '', key: '' });
  const list = obs.tools ?? [];
  const add = () => {
    const tool: Tool = { id: f.id, product: f.product, mode: f.mode as Tool['mode'], ...(f.seeded ? { seeded_from: f.seeded } : {}), ...(f.key ? { auth: { key: f.key } } : {}) };
    onChange({ ...obs, tools: addTo(list, tool) });
    setF({ id: '', product: '', mode: 'stand-in', seeded: '', key: '' });
  };
  return (
    <Card title="World tool declarations" description="Datadog, Grafana, Splunk, Sentry, PagerDuty… Saved as world configuration; adding a declaration does not connect or verify a tool.">
      <ItemRows items={list} empty="No observability tools yet." onRemove={(i) => onChange({ ...obs, tools: removeAt(list, i) })}
        render={(t) => <span><span className="font-medium">{t.product}</span> <span className="text-faint">· {t.mode === 'real' ? 'real, tagged by run' : 'stand-in'}</span></span>} />
      <div className="grid gap-3 border-t border-line p-[18px] md:grid-cols-2">
        <TextField label="Tool id" mono value={f.id} onChange={(v) => setF({ ...f, id: v })} />
        <TextField label="Product" value={f.product} onChange={(v) => setF({ ...f, product: v })} placeholder="datadog" />
        <ChoiceField label="Mode" value={f.mode} options={[{ value: 'stand-in', label: 'Stand-in' }, { value: 'real', label: 'Connect the real tool' }]} onChange={(v) => setF({ ...f, mode: v })} />
        {f.mode === 'stand-in'
          ? <TextField label="Seeded from" mono value={f.seeded} onChange={(v) => setF({ ...f, seeded: v })} placeholder="monitors.json" />
          : <TextField label="Tool key name" mono value={f.key} onChange={(v) => setF({ ...f, key: v })} placeholder="A name from the key store, never the key" />}
      </div>
      <div className="flex justify-end border-t border-line px-[18px] py-2.5"><Button disabled={!f.id || !f.product} onClick={add}>Add tool</Button></div>
    </Card>
  );
}
