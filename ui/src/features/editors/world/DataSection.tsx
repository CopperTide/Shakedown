// What every run starts from: files, databases and connector imports.
import { Card } from '@/components/ui/card';
import { Toggle } from '@/components/ui/toggle';
import type { DataSource, WorldBody } from '@/model/project';
import { addTo, removeAt, setIn } from '../edit';
import { SectionHead } from '../fields';
import AddDataSource from './AddDataSource';

function Source({ source, onChange, onRemove }: { source: DataSource; onChange: (s: DataSource) => void; onRemove: () => void }) {
  const name = source.database ?? source.file ?? source.import ?? '';
  return (
    <Card title={name} description={source.database ? `Database · ${source.engine ?? ''} ${source.version ?? ''}` : source.file ? 'File' : 'Connector import'}
      actions={<button type="button" className="text-xs text-alarm" onClick={onRemove}>Remove</button>}>
      {source.database && (
        <div className="grid gap-3 p-[18px]">
          <Toggle label="Masked" description="Personal columns are masked before the snapshot is stored." checked={!!source.masked} onChange={(v) => onChange(setIn(source, ['masked'], v || undefined))} />
          <Toggle label="Every run starts from this" description="Each run gets its own copy; runs never see each other's writes." checked={!!source.default_state} onChange={(v) => onChange(setIn(source, ['default_state'], v || undefined))} />
        </div>
      )}
    </Card>
  );
}

export default function DataSection({ world, onChange }: { world: WorldBody; onChange: (w: WorldBody) => void }) {
  const list = world.data ?? [];
  return (
    <div className="grid gap-4">
      <SectionHead title="Data" about="What the world starts with. Every run restores this state, so runs are repeatable." />
      {list.map((s, i) => (
        <Source key={i} source={s} onChange={(n) => onChange({ ...world, data: setIn(list, [i], n) })} onRemove={() => onChange({ ...world, data: removeAt(list, i) })} />
      ))}
      {list.length === 0 && <p className="text-sm text-faint">No data yet. Add files, a database or a connector import.</p>}
      <AddDataSource onAdd={(s) => onChange({ ...world, data: addTo(list, s) })} />
    </div>
  );
}
