// What every run starts from: files in the world's folder (uploaded here, viewed as tables, edited
// in place), databases and connector imports. A file marked "Every run starts from this" is given
// to every part of each run (docs/protocol.md, "World data").
import { useRef, useState } from 'react';
import { deleteWorldData, putWorldData } from '@/api/project';
import { base64Of } from '@/api/runtimes';
import { Card } from '@/components/ui/card';
import { Toggle } from '@/components/ui/toggle';
import { errorMessage } from '@/lib/errors';
import type { DataSource, WorldBody } from '@/model/project';
import { addTo, removeAt, setIn } from '../edit';
import { SectionHead } from '../fields';
import AddDataSource, { DATA_FILES } from './AddDataSource';
import DataFileViewer from './DataFileViewer';

const STARTS = 'Every run starts from this';

/** Replace (a new upload of the same file) and Delete file, for a file source. */
function FileActions({ world, file, onDeleted }: { world: string; file: string; onDeleted: () => void }) {
  const input = useRef<HTMLInputElement>(null);
  const [note, setNote] = useState('');
  const replace = async (f: File | undefined) => {
    if (!f) return;
    try { await putWorldData(world, file, { data: await base64Of(f) }); setNote('Replaced.'); } catch (e) { setNote(errorMessage(e)); }
  };
  const remove = async () => {
    try { await deleteWorldData(world, file); onDeleted(); } catch (e) { setNote(errorMessage(e)); }
  };
  return (
    <>
      {note && <span role="status" className="text-xs text-muted">{note}</span>}
      <input ref={input} type="file" accept={DATA_FILES} hidden aria-label={`Replace ${file}`} onChange={(e) => { void replace(e.target.files?.[0]); e.target.value = ''; }} />
      <button type="button" className="text-xs text-primary" onClick={() => input.current?.click()}>Replace</button>
      <button type="button" className="text-xs text-alarm" onClick={() => { void remove(); }}>Delete file</button>
    </>
  );
}

/** What kind of source it is, on one line. */
function kindOf(s: DataSource): string {
  if (s.database) return `Database · ${s.engine ?? ''} ${s.version ?? ''}`;
  if (s.file) return s.seeds ? `File · seeds ${s.seeds}` : 'File';
  return 'Connector import';
}

/** Masked (a database) and Every run starts from this (a database or a file). */
function Toggles({ source, onChange }: { source: DataSource; onChange: (s: DataSource) => void }) {
  if (!source.database && !source.file) return null;
  const about = source.file ? "Each run's parts get this file when they start; an edit reaches the next run." : "Marks the source as part of each run's starting state.";
  return (
    <div className="grid gap-3 p-[18px]">
      {source.database && <Toggle label="Masked" description="Marks personal columns to mask before the snapshot is kept." checked={!!source.masked} onChange={(v) => onChange(setIn(source, ['masked'], v || undefined))} />}
      <Toggle label={STARTS} description={about} checked={!!source.default_state} onChange={(v) => onChange(setIn(source, ['default_state'], v || undefined))} />
    </div>
  );
}

function Source({ world, source, onChange, onRemove }: { world: string; source: DataSource; onChange: (s: DataSource) => void; onRemove: () => void }) {
  const name = source.database ?? source.file ?? source.import ?? '';
  return (
    <Card title={name} description={kindOf(source)}
      actions={<div className="flex items-center gap-3">
        {source.file && <><DataFileViewer world={world} source={source.file} /><FileActions world={world} file={source.file} onDeleted={onRemove} /></>}
        <button type="button" className="text-xs text-alarm" onClick={onRemove} title="Remove it from the world's data (the file stays)">Remove</button>
      </div>}>
      <Toggles source={source} onChange={onChange} />
    </Card>
  );
}

export default function DataSection({ id, world, onChange }: { id: string; world: WorldBody; onChange: (w: WorldBody) => void }) {
  const list = world.data ?? [];
  return (
    <div className="grid gap-4">
      <SectionHead title="Data" about="What the world starts with. Upload files (JSON, CSV, YAML or text) into the world's folder; those marked “Every run starts from this” reach every part of each run, so an edit changes what the next run starts from. Save the world to keep the list." />
      {list.map((s, i) => (
        <Source key={i} world={id} source={s} onChange={(n) => onChange({ ...world, data: setIn(list, [i], n) })} onRemove={() => onChange({ ...world, data: removeAt(list, i) })} />
      ))}
      {list.length === 0 && <p className="text-sm text-faint">No data yet. Upload a file, or add a database or a connector import.</p>}
      <AddDataSource world={id} taken={list.flatMap((s) => (s.file ? [s.file] : []))} onAdd={(s) => onChange({ ...world, data: addTo(list, s) })} />
    </div>
  );
}
