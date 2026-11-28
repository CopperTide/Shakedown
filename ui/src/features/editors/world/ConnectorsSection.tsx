// Saved connector declarations, their interfaces, and the vendor discovery catalog.
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import type { Connector, WorldBody } from '@/model/project';
import { addTo, removeAt } from '../edit';
import { ItemRows, SectionHead } from '../fields';
import AddConnector from './AddConnector';
import ConnectorCatalog from './catalog/ConnectorCatalog';
import ConnectorBrand from './catalog/ConnectorBrand';
import ConnectorDetails from './catalog/ConnectorDetails';
import { catalogEntry } from './catalog/search';
export { connectorOf } from './AddConnector';

function ConnectorRow({ connector: c, onInspect }: { connector: Connector; onInspect: () => void }) {
  const entry = catalogEntry(c.product, c.label);
  return <button onClick={onInspect} className="flex w-full items-center gap-3 rounded text-left outline-none focus-visible:ring-2 focus-visible:ring-accent" aria-label={`Inspect ${c.label}`}>
    <ConnectorBrand entry={entry} name={c.label} /><span className="min-w-0 flex-1"><span className="block font-medium">{c.label}</span><span className="mt-1 block text-xs text-muted">{[c.mcp && 'MCP', c.openapi && 'OpenAPI', c.product && 'Product', c.mode].filter(Boolean).join(' · ')}</span></span><span className="text-xs text-muted">Details →</span>
  </button>;
}

export default function ConnectorsSection({ world, onChange }: { world: WorldBody; onChange: (w: WorldBody) => void }) {
  const list = world.connectors ?? [];
  const [catalog, setCatalog] = useState(false);
  const [inspect, setInspect] = useState<Connector | null>(null);
  const [problem, setProblem] = useState('');
  const add = (c: Connector) => {
    if (list.some((item) => item.id === c.id)) { const error = `Connector id “${c.id}” is already in this world.`; setProblem(error); return error; }
    setProblem(''); onChange({ ...world, connectors: addTo(list, c) });
  };
  return <div className="grid gap-4">
    <SectionHead title="Connectors" about="Inspect the systems in this world, or discover enterprise APIs and MCP servers in the catalog." actions={<Button onClick={() => setCatalog(true)}>Browse catalog</Button>} />
    {problem && <p role="alert" className="text-sm text-alarm">{problem}</p>}
    <Card title={`In this world · ${list.length}`} hint={world.package ? `The runtime ${world.package} plays the stand-ins; real systems need their configuration.` : 'Saved configuration. Stand-ins need a runtime that plays them: pick one in Runtime.'}>
      <ItemRows items={list} empty="No connectors yet." emptyHint="Browse the catalog, or add a custom API below." onRemove={(i) => onChange({ ...world, connectors: removeAt(list, i) })} render={(c) => <ConnectorRow connector={c} onInspect={() => setInspect(c)} />} />
    </Card>
    <AddConnector onAdd={add} runtime={world.package} />
    <ConnectorCatalog open={catalog} onClose={() => setCatalog(false)} onAdd={add} />
    <Dialog open={!!inspect} onOpenChange={(open) => { if (!open) setInspect(null); }}><DialogContent title="Connector details" className="overflow-y-auto">{inspect && <ConnectorDetails connector={inspect} entry={catalogEntry(inspect.product, inspect.label)} />}</DialogContent></Dialog>
  </div>;
}
