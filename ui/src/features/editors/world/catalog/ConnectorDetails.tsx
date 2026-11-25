import { ExternalLink, Cable, Code2 } from 'lucide-react';
import type { Connector } from '@/model/project';
import { Button } from '@/components/ui/button';
import SchemaInspector from './SchemaInspector';
import ConnectorBrand from './ConnectorBrand';
import type { CatalogEntry } from './catalog';

function Configuration({ connector: c }: { connector: Connector }) {
  const fields = [['Identifier', c.id], ['Run mode', c.mode], ['Product', c.product], ['MCP server', c.mcp], ['OpenAPI specification', c.openapi], ['Credential reference', c.auth?.key ? `Key store: ${c.auth.key}` : c.auth?.env ? `Environment: ${c.auth.env}` : 'None']];
  return <section className="grid gap-3"><h3 className="font-semibold">Saved configuration</h3><dl className="grid gap-3 text-sm">
    {fields.filter(([, v]) => v).map(([label, value]) => <div key={label}><dt className="text-xs text-muted">{label}</dt><dd className="mt-1 break-all font-mono text-xs">{value}</dd></div>)}
  </dl><p className="text-xs text-muted">Configuration is saved with the world. It does not verify connectivity or enumerate live server tools.</p></section>;
}

function DetailHeader({ entry, connector }: { entry?: CatalogEntry; connector?: Connector }) {
  return <header className="flex items-center gap-3"><ConnectorBrand entry={entry} name={connector?.label} /><div><h2 className="text-lg font-semibold">{connector?.label ?? entry?.name}</h2><p className="text-xs text-muted">{entry?.vendor ?? 'Custom connector'} · {entry?.category ?? 'API & MCP'}</p></div></header>;
}

export default function ConnectorDetails({ entry, connector, onChoose }: { entry?: CatalogEntry; connector?: Connector; onChoose?: () => void }) {
  return <div className="flex min-w-0 flex-col gap-6 p-5">
    <DetailHeader entry={entry} connector={connector} />
    {entry && <>
      <div className="rounded-md border border-line bg-raised p-3 text-xs"><strong>Planned adapter</strong><p className="mt-1 text-muted">Explore the proposed integration scope and configure a world declaration. A native CopperTide adapter is not yet verified.</p></div>
      <section><h3 className="mb-2 flex items-center gap-2 text-sm font-semibold"><Cable className="size-4" />Interfaces</h3><div className="flex flex-wrap gap-2">{entry.protocols.map((p) => <span key={p} className="rounded border border-line px-2 py-1 text-xs">{p}</span>)}</div></section>
      <section><h3 className="mb-2 flex items-center gap-2 text-sm font-semibold"><Code2 className="size-4" />API areas to integrate</h3><ul className="divide-y divide-line rounded-md border border-line">{entry.areas.map((a) => <li key={a} className="px-3 py-2 text-sm">{a}</li>)}</ul><p className="mt-2 text-xs text-muted">Scope preview, not a list of installed agent tools. Exact operations and permissions depend on the vendor and your deployment.</p></section>
      <section><h3 className="mb-2 text-sm font-semibold">Industries</h3><p className="text-sm text-muted">{entry.industries.join(' · ')}</p></section>
      <a href={entry.docs} target="_blank" rel="noreferrer" className="flex items-center gap-2 text-sm text-accent underline">Vendor API documentation<ExternalLink className="size-3" /></a>
    </>}
    {connector && <Configuration connector={connector} />}
    {!entry && <p className="text-sm text-muted">Inspect the configured MCP endpoint or OpenAPI file below. Live tool discovery has not been performed.</p>}
    <SchemaInspector />
    {onChoose && <Button onClick={onChoose}>Use configuration template</Button>}
  </div>;
}
