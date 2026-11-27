import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import type { Connector } from '@/model/project';
import { ChoiceField, TextField } from '../fields';

/** How runs use a connector; a stand-in is played by the world's runtime, when it has one. */
export function modes(runtime?: string) {
  const standIn = runtime ? `Stand-in (played by the runtime ${runtime})` : 'Stand-in (pick a runtime that plays it)';
  return [{ value: 'real', label: 'Real system (requires configuration)' }, { value: 'replay', label: 'Record once, then replay' }, { value: 'stand-in', label: standIn }] as { value: Connector['mode']; label: string }[];
}

/** The connector the form describes, without empty fields. */
export function connectorOf(f: Record<string, string>): Connector {
  f = Object.fromEntries(Object.entries(f).map(([key, value]) => [key, value.trim()]));
  const c: Connector = { id: f.id, label: f.label, mode: (f.mode || 'real') as Connector['mode'] };
  if (f.product) c.product = f.product;
  if (f.mcp) c.mcp = f.mcp;
  if (f.openapi) c.openapi = f.openapi;
  if (f.key) c.auth = { key: f.key };
  else if (f.env) c.auth = { env: f.env };
  return c;
}

export default function AddConnector({ onAdd, initial = {}, runtime }: { onAdd: (c: Connector) => string | void; initial?: Record<string, string>; runtime?: string }) {
  const [f, setF] = useState<Record<string, string>>({ mode: 'real', ...initial });
  const [error, setError] = useState('');
  const field = (k: string) => ({ value: f[k] ?? '', onChange: (v: string) => setF({ ...f, [k]: v }) });
  return (
    <Card title="Add a connector">
      {error && <p role="alert" className="px-4 pt-3 text-sm text-alarm">{error}</p>}
      <div className="grid gap-3 p-[18px] md:grid-cols-2">
        <TextField label="Connector id" mono placeholder="mes" {...field('id')} />
        <TextField label="Connector name" placeholder="Larkspur MES" {...field('label')} />
        <ChoiceField label="How runs use it" value={(f.mode ?? 'real') as Connector['mode']} options={modes(runtime)} onChange={(v) => setF({ ...f, mode: v })} />
        <TextField label="Product" placeholder="sap-s4hana (product identifier)" {...field('product')} />
        <TextField label="MCP server URL" mono placeholder="https://…/mcp" {...field('mcp')} />
        <TextField label="OpenAPI spec file" mono placeholder="carrier-rates.openapi.yaml" {...field('openapi')} />
        <TextField label="Key-store name" mono placeholder="A name from the key store, never the key" {...field('key')} />
        <TextField label="Or environment variable" mono placeholder="CARRIER_TOKEN" {...field('env')} />
      </div>
      <div className="flex justify-end border-t border-line px-[18px] py-2.5">
        <Button disabled={!f.id?.trim() || !f.label?.trim()} onClick={() => { const problem = onAdd(connectorOf(f)); setError(problem || ''); if (!problem) setF({ mode: 'real', ...initial }); }}>Add connector</Button>
      </div>
    </Card>
  );
}

