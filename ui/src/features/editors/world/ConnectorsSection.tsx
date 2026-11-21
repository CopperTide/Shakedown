// The systems agents work against: each a product, an MCP server or an API, real, recorded or a
// stand-in, with its credential named, never written.
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import type { Connector, WorldBody } from '@/model/project';
import { addTo, removeAt } from '../edit';
import { ChoiceField, ItemRows, SectionHead, TextField } from '../fields';

const MODES = [{ value: 'real', label: 'Real system, sandboxed' }, { value: 'replay', label: 'Record once, then replay' }, { value: 'stand-in', label: 'Built-in stand-in' }] as const;

/** The connector the form describes, without empty fields. */
export function connectorOf(f: Record<string, string>): Connector {
  const c: Connector = { id: f.id, label: f.label, mode: (f.mode || 'real') as Connector['mode'] };
  if (f.product) c.product = f.product;
  if (f.mcp) c.mcp = f.mcp;
  if (f.openapi) c.openapi = f.openapi;
  if (f.key) c.auth = { key: f.key };
  else if (f.env) c.auth = { env: f.env };
  return c;
}

function AddConnector({ onAdd }: { onAdd: (c: Connector) => void }) {
  const [f, setF] = useState<Record<string, string>>({ mode: 'real' });
  const field = (k: string) => ({ value: f[k] ?? '', onChange: (v: string) => setF({ ...f, [k]: v }) });
  return (
    <Card title="Add a connector">
      <div className="grid gap-3 p-[18px] md:grid-cols-2">
        <TextField label="Connector id" mono placeholder="mes" {...field('id')} />
        <TextField label="Connector name" placeholder="Larkspur MES" {...field('label')} />
        <ChoiceField label="How runs use it" value={(f.mode ?? 'real') as Connector['mode']} options={[...MODES]} onChange={(v) => setF({ ...f, mode: v })} />
        <TextField label="Product" placeholder="sap-s4hana (a built-in product connector)" {...field('product')} />
        <TextField label="MCP server URL" mono placeholder="https://…/mcp" {...field('mcp')} />
        <TextField label="OpenAPI spec file" mono placeholder="carrier-rates.openapi.yaml" {...field('openapi')} />
        <TextField label="Key-store name" mono placeholder="A name from the key store, never the key" {...field('key')} />
        <TextField label="Or environment variable" mono placeholder="CARRIER_TOKEN" {...field('env')} />
      </div>
      <div className="flex justify-end border-t border-line px-[18px] py-2.5">
        <Button disabled={!f.id || !f.label} onClick={() => { onAdd(connectorOf(f)); setF({ mode: 'real' }); }}>Add connector</Button>
      </div>
    </Card>
  );
}

export default function ConnectorsSection({ world, onChange }: { world: WorldBody; onChange: (w: WorldBody) => void }) {
  const list = world.connectors ?? [];
  return (
    <div className="grid gap-4">
      <SectionHead title="Connectors" about="The software products agents work against. Runs use a sandboxed copy; real systems are never written to." />
      <Card>
        <ItemRows items={list} empty="No connectors yet." onRemove={(i) => onChange({ ...world, connectors: removeAt(list, i) })}
          render={(c) => <span><span className="font-medium">{c.label}</span> <span className="font-mono text-2xs text-faint">{c.id} · {c.mode}</span></span>} />
      </Card>
      <AddConnector onAdd={(c) => onChange({ ...world, connectors: addTo(list, c) })} />
    </div>
  );
}
