// Adding data to a world: a file uploaded into its folder (`data/<name>`, marked so every run
// starts from it unless unticked), or a database or a connector import described by name.
import { useState } from 'react';
import { putWorldData } from '@/api/project';
import { base64Of } from '@/api/runtimes';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Hint } from '@/components/ui/hint';
import { Toggle } from '@/components/ui/toggle';
import { errorMessage } from '@/lib/errors';
import type { DataSource } from '@/model/project';
import { ChoiceField, TextField } from '../fields';

type Kind = 'file' | 'database' | 'import';
const KINDS = [{ value: 'file', label: 'A file (upload)' }, { value: 'database', label: 'A database' }, { value: 'import', label: 'A connector import' }] as const;

/** The files a world's data takes. */
export const DATA_FILES = '.json,.csv,.tsv,.yaml,.yml,.txt,.md';

/** The source the form describes, without empty fields. */
export function sourceOf(kind: Kind, f: Record<string, string>): DataSource {
  const s: DataSource = { [kind]: f.name };
  for (const k of ['engine', 'version', 'snapshot', 'seeds', 'from'] as const) if (f[k]) s[k] = f[k];
  return s;
}

/** Where an uploaded file goes in the world's folder. */
export const uploadPath = (name: string) => `data/${name.split(/[\\/]/).pop() ?? name}`;

/** The fields of each kind: a file to upload (and what it seeds, and whether runs start from it), else a name. */
function Fields({ kind, field, onFile, starts, onStarts }: {
  kind: Kind; field: (k: string) => { value: string; onChange: (v: string) => void };
  onFile: (f: File | null) => void; starts: boolean; onStarts: (v: boolean) => void;
}) {
  if (kind === 'file') {
    return (
      <>
        <label className="grid gap-1 text-sm"><span className="text-muted"><Hint hint="JSON, CSV, YAML or text; at most 10 MiB.">File</Hint></span><input aria-label="Data file" type="file" accept={DATA_FILES} className="text-sm" onChange={(e) => onFile(e.target.files?.[0] ?? null)} /></label>
        <TextField label="Seeds connector" hint="Optional: the connector this file fills." mono placeholder="erp" {...field('seeds')} />
        <Toggle label="Every run starts from this" description="Each run's parts get the file when they start." checked={starts} onChange={onStarts} />
      </>
    );
  }
  return (
    <>
      <TextField label="Name" placeholder="larkspur-prod" {...field('name')} />
      {kind === 'database' && <><TextField label="Engine" placeholder="postgres" {...field('engine')} /><TextField label="Version" mono placeholder="15.6" {...field('version')} /></>}
      {kind === 'import' && <TextField label="From connector" mono placeholder="crm" {...field('from')} />}
    </>
  );
}

/** Uploads `file` into the world's folder; the source it makes, or null when it is listed already. */
async function upload(world: string, file: File, f: Record<string, string>, starts: boolean, taken: string[]): Promise<DataSource | null> {
  const at = uploadPath(file.name);
  await putWorldData(world, at, { data: await base64Of(file) });
  if (taken.includes(at)) return null;
  return { ...sourceOf('file', { ...f, name: at }), ...(starts ? { default_state: true } : {}) };
}

export default function AddDataSource({ world, taken = [], onAdd }: { world: string; taken?: string[]; onAdd: (s: DataSource) => void }) {
  const [kind, setKind] = useState<Kind>('file');
  const [f, setF] = useState<Record<string, string>>({});
  const [file, setFile] = useState<File | null>(null);
  const [starts, setStarts] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const field = (k: string) => ({ value: f[k] ?? '', onChange: (v: string) => setF({ ...f, [k]: v }) });
  const add = async () => {
    setError(''); setBusy(true);
    try {
      const made = kind === 'file' ? (file ? await upload(world, file, f, starts, taken) : null) : sourceOf(kind, f);
      if (made) onAdd(made);
      setF({}); setFile(null);
    } catch (e) { setError(errorMessage(e)); } finally { setBusy(false); }
  };
  const isFile = kind === 'file';
  return (
    <Card title="Add data">
      <div className="grid gap-3 p-[18px] md:grid-cols-2">
        <ChoiceField label="Kind of data" value={kind} options={[...KINDS]} onChange={(k) => { setKind(k); setError(''); }} />
        <Fields kind={kind} field={field} onFile={setFile} starts={starts} onStarts={setStarts} />
      </div>
      {error && <p role="alert" className="px-[18px] pb-2 text-sm text-alarm">{error}</p>}
      <div className="flex items-center justify-end gap-3 border-t border-line px-[18px] py-2.5">
        {isFile && file && <span className="text-xs text-muted">Uploads to <span className="font-mono">{uploadPath(file.name)}</span></span>}
        <Button disabled={(isFile ? !file : !f.name) || busy} onClick={() => { void add(); }}>{busy ? 'Uploading…' : isFile ? 'Upload' : 'Add data'}</Button>
      </div>
    </Card>
  );
}
