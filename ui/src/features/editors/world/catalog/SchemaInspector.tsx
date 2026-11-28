import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Hint } from '@/components/ui/hint';
import { Input, Textarea } from '@/components/ui/input';
import { schemaOperations, type ApiOperation } from './schema';

export default function SchemaInspector() {
  const [source, setSource] = useState('');
  const [operations, setOperations] = useState<ApiOperation[] | null>(null);
  const [error, setError] = useState('');
  const [query, setQuery] = useState('');
  const inspect = (text: string) => {
    try { setOperations(schemaOperations(text)); setError(''); }
    catch (e) { setOperations(null); setError(e instanceof Error ? e.message : 'Unable to read schema.'); }
  };
  const visible = operations?.filter((o) => `${o.name} ${o.description}`.toLowerCase().includes(query.toLowerCase()));
  return <details className="rounded-md border border-line p-3"><summary className="cursor-pointer text-sm font-semibold">Inspect operations</summary>
    <div className="mt-3 grid gap-3">
      <label className="grid gap-1 text-xs text-muted"><Hint hint="An OpenAPI JSON file or an MCP tools/list response. It stays in this browser; no tools are called.">Schema JSON file</Hint><input type="file" accept=".json,application/json" className="text-xs" onChange={async (e) => { const file = e.target.files?.[0]; if (!file) return; if (file.size > 2 * 1024 * 1024) { setError('Choose a schema smaller than 2 MB.'); return; } try { const text = await file.text(); setSource(text); inspect(text); } catch { setError('Unable to read this file.'); } }} /></label>
      <Textarea aria-label="OpenAPI or MCP schema JSON" placeholder="Or paste schema JSON…" rows={3} value={source} onChange={(e) => { setSource(e.target.value); setOperations(null); }} />
      <Button size="sm" disabled={!source.trim()} onClick={() => inspect(source)}>Inspect schema</Button>
      {error && <p role="alert" className="text-xs text-alarm">{error}</p>}
      {operations && <><Input aria-label="Search API operations" placeholder="Find an operation or tool…" value={query} onChange={(e) => setQuery(e.target.value)} /><p role="status" className="text-xs text-muted">{visible?.length} of {operations.length} operations</p><div className="max-h-80 overflow-y-auto">{visible?.map((o, i) => <details key={`${o.name}-${i}`} className="border-t border-line py-2"><summary className="cursor-pointer break-all font-mono text-xs">{o.name}</summary><p className="my-2 text-xs text-muted">{o.description}</p><pre className="max-h-64 overflow-auto rounded bg-sunken p-2 text-2xs">{JSON.stringify(o.schema, null, 2)}</pre></details>)}</div></>}
    </div>
  </details>;
}
