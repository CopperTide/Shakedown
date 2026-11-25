// What every run starts from: files, databases and connector imports.
import { Card } from '@/components/ui/card';
import { Toggle } from '@/components/ui/toggle';
import type { DataSource, WorldBody } from '@/model/project';
import { addTo, removeAt, setIn } from '../edit';
import { SectionHead } from '../fields';
import AddDataSource from './AddDataSource';
import DataFileViewer from './DataFileViewer';

function Source({ world, source, onChange, onRemove }: { world: string; source: DataSource; onChange: (s: DataSource) => void; onRemove: () => void }) {
  const name = source.database ?? source.file ?? source.import ?? '';
  return (
    <Card title={name} description={source.database ? `Database · ${source.engine ?? ''} ${source.version ?? ''}` : source.file ? 'File' : 'Connector import'}
      actions={<>
        {source.file && <DataFileViewer world={world} source={source.file} />}
        <button type="button" className="text-xs text-alarm" onClick={onRemove}>Remove</button>
      </>}>
      {source.database && (
        <div className="grid gap-3 p-[18px]">
          <Toggle label="Masked" description="Marks personal columns to mask. Saved with the world; runs use it once worlds run from their own files." checked={!!source.masked} onChange={(v) => onChange(setIn(source, ['masked'], v || undefined))} />
          <Toggle label="Every run starts from this" description="Marks the source as part of each run's starting state. Saved with the world; runs use it once worlds run from their own files." checked={!!source.default_state} onChange={(v) => onChange(setIn(source, ['default_state'], v || undefined))} />
        </div>
      )}
    </Card>
  );
}

export default function DataSection({ id, world, onChange }: { id: string; world: WorldBody; onChange: (w: WorldBody) => void }) {
  const list = world.data ?? [];
  return (
    <div className="grid gap-4">
      <SectionHead title="Data" about="What the world starts with. Saved with the world. Runs use it once worlds run from their own files." />
      {list.map((s, i) => (
        <Source key={i} world={id} source={s} onChange={(n) => onChange({ ...world, data: setIn(list, [i], n) })} onRemove={() => onChange({ ...world, data: removeAt(list, i) })} />
      ))}
      {list.length === 0 && <p className="text-sm text-faint">No data yet. Add files, a database or a connector import.</p>}
      <AddDataSource onAdd={(s) => onChange({ ...world, data: addTo(list, s) })} />
    </div>
  );
}
