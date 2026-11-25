import { useMemo, useState, useRef, useEffect, type RefObject } from 'react';
import { Search, ArrowLeft } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import { Input, NativeSelect } from '@/components/ui/input';
import type { Connector } from '@/model/project';
import AddConnector from '../AddConnector';
import { CATALOG, type CatalogEntry } from './catalog';
import ConnectorBrand from './ConnectorBrand';
import ConnectorDetails from './ConnectorDetails';
import { EMPTY_FILTER, findConnectors, type CatalogFilter } from './search';

function Filters({ filter, set, inputRef }: { filter: CatalogFilter; set: (f: CatalogFilter) => void; inputRef: RefObject<HTMLInputElement | null> }) {
  const options = { category: CATALOG.map((c) => c.category), industry: CATALOG.flatMap((c) => c.industries), vendor: CATALOG.map((c) => c.vendor), protocol: CATALOG.flatMap((c) => c.protocols) };
  const allLabels = { category: 'All categories', industry: 'All industries', vendor: 'All companies', protocol: 'All interfaces' };
  const labels = { category: 'Category', industry: 'Industry', vendor: 'Company', protocol: 'Interface' };
  return <div className="grid gap-3 border-b border-line bg-raised/40 p-4">
    <label className="relative"><Search className="absolute left-3 top-2.5 size-4 text-muted" /><Input ref={inputRef} autoFocus aria-label="Search connector catalog" placeholder="Search tools, companies, APIs, or workflows…" className="pl-9" value={filter.query} onChange={(e) => set({ ...filter, query: e.target.value })} /></label>
    <div className="grid grid-cols-2 gap-2 lg:grid-cols-4">{(Object.keys(options) as (keyof typeof options)[]).map((key) => <label key={key} className="grid gap-1 text-xs text-muted">{labels[key]}<NativeSelect aria-label={labels[key]} value={filter[key]} onChange={(e) => set({ ...filter, [key]: e.target.value })}><option value="">{allLabels[key]}</option>{[...new Set(options[key])].sort().map((v) => <option key={v}>{v}</option>)}</NativeSelect></label>)}</div>
  </div>;
}

function CatalogCard({ entry, selected, onSelect }: { entry: CatalogEntry; selected: boolean; onSelect: () => void }) {
  return <button onClick={onSelect} aria-pressed={selected} className={`flex w-full gap-3 rounded-lg border p-4 text-left outline-none focus-visible:ring-2 focus-visible:ring-accent ${selected ? 'border-accent bg-accent-soft' : 'border-line bg-surface hover:bg-hover'}`}>
    <ConnectorBrand entry={entry} /><div className="min-w-0 flex-1"><h3 className="text-sm font-semibold">{entry.name}</h3><p className="mt-0.5 text-xs text-muted">{entry.vendor} · {entry.category}</p><p className="mt-2 text-xs text-muted">{entry.areas.join(' · ')}</p><div className="mt-3 flex flex-wrap gap-2 text-2xs text-muted"><span className="rounded bg-sunken px-1.5 py-0.5">Planned</span><span>{entry.protocols.join(' · ')}</span></div></div>
  </button>;
}

function Browse({ onChoose }: { onChoose: (e: CatalogEntry) => void }) {
  const [filter, setFilter] = useState(EMPTY_FILTER);
  const [selected, setSelected] = useState<string | null>(null);
  const entries = useMemo(() => findConnectors(filter), [filter]);
  const entry = entries.find((e) => e.id === selected);
  const details = useRef<HTMLElement>(null);
  const search = useRef<HTMLInputElement>(null);
  useEffect(() => { if (selected) details.current?.focus(); else search.current?.focus(); }, [selected]);
  return <><Filters filter={filter} set={setFilter} inputRef={search} /><div className="flex items-center justify-between border-b border-line px-4 py-2 text-xs text-muted"><span role="status">{entries.length} of {CATALOG.length} connectors</span><Button size="sm" variant="ghost" onClick={() => { setFilter(EMPTY_FILTER); setSelected(null); }}>Clear filters</Button></div>
    <div className="grid min-h-0 flex-1 overflow-y-auto lg:grid-cols-[1.2fr_1fr]">
      <div className={`${entry ? 'hidden lg:grid' : 'grid'} content-start gap-2 p-4 lg:overflow-y-auto`}>{entries.map((e) => <CatalogCard key={e.id} entry={e} selected={e.id === selected} onSelect={() => setSelected(e.id)} />)}{!entries.length && <p className="py-8 text-center text-sm text-muted">No matching connectors. Clear a filter or add a custom API / MCP connector.</p>}</div>
      <aside ref={details} tabIndex={-1} aria-label="Connector details" className="border-t border-line bg-raised/30 lg:overflow-y-auto lg:border-l lg:border-t-0">{entry && <Button variant="ghost" className="m-3 lg:hidden" onClick={() => setSelected(null)}><ArrowLeft className="size-4" />Back to results</Button>}{entry ? <ConnectorDetails key={entry.id} entry={entry} onChoose={() => onChoose(entry)} /> : <div className="p-8 text-sm text-muted"><h3 className="mb-2 font-semibold text-fg">Find the right connection</h3>Select a connector to inspect its interfaces, proposed scope, industries, and vendor documentation.</div>}</aside>
    </div></>;
}

export default function ConnectorCatalog({ open, onClose, onAdd }: { open: boolean; onClose: () => void; onAdd: (c: Connector) => string | void }) {
  const [chosen, setChosen] = useState<CatalogEntry | null>(null);
  return <Dialog open={open} onOpenChange={(v) => { if (!v) { setChosen(null); onClose(); } }}><DialogContent title="Connector catalog" description="Find enterprise tools by company, industry, workflow, or interface. Planned adapters are labeled." className="top-[5vh] h-[90vh] max-h-[90vh] w-[min(1120px,calc(100vw-24px))]">
    {chosen ? <div className="overflow-y-auto p-4"><Button variant="ghost" onClick={() => setChosen(null)}><ArrowLeft className="size-4" />Back to catalog</Button><p className="my-3 text-sm text-muted">{chosen.name} is a planned adapter. This creates a configuration declaration, not a verified connection.</p><AddConnector initial={{ id: chosen.id, label: chosen.name, product: chosen.id }} onAdd={(c) => { const problem = onAdd(c); if (problem) return problem; setChosen(null); onClose(); }} /></div> : <Browse onChoose={setChosen} />}
  </DialogContent></Dialog>;
}
