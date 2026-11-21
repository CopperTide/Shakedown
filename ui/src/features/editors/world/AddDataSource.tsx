import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import type { DataSource } from '@/model/project';
import { ChoiceField, TextField } from '../fields';

type Kind = 'file' | 'database' | 'import';
const KINDS = [{ value: 'file', label: 'A file' }, { value: 'database', label: 'A database' }, { value: 'import', label: 'A connector import' }] as const;

/** The source the form describes, without empty fields. */
export function sourceOf(kind: Kind, f: Record<string, string>): DataSource {
  const s: DataSource = { [kind]: f.name };
  for (const k of ['engine', 'version', 'snapshot', 'seeds', 'from'] as const) if (f[k]) s[k] = f[k];
  return s;
}

export default function AddDataSource({ onAdd }: { onAdd: (s: DataSource) => void }) {
  const [kind, setKind] = useState<Kind>('file');
  const [f, setF] = useState<Record<string, string>>({});
  const field = (k: string) => ({ value: f[k] ?? '', onChange: (v: string) => setF({ ...f, [k]: v }) });
  return (
    <Card title="Add data">
      <div className="grid gap-3 p-[18px] md:grid-cols-2">
        <ChoiceField label="Kind of data" value={kind} options={[...KINDS]} onChange={setKind} />
        <TextField label="Name" placeholder={kind === 'file' ? 'suppliers.csv' : 'larkspur-prod'} {...field('name')} />
        {kind === 'database' && <><TextField label="Engine" placeholder="postgres" {...field('engine')} /><TextField label="Version" mono placeholder="15.6" {...field('version')} /></>}
        {kind === 'file' && <TextField label="Seeds connector" mono placeholder="erp" {...field('seeds')} />}
        {kind === 'import' && <TextField label="From connector" mono placeholder="crm" {...field('from')} />}
      </div>
      <div className="flex justify-end border-t border-line px-[18px] py-2.5">
        <Button disabled={!f.name} onClick={() => { onAdd(sourceOf(kind, f)); setF({}); }}>Add data</Button>
      </div>
    </Card>
  );
}
