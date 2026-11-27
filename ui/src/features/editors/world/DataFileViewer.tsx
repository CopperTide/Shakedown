// A world's data file: JSON records as tables (one per section of a database), where a value can
// be changed in place (saved to the file at once); other files as text. A folder lists its files.
import { useEffect, useState } from 'react';
import { putWorldData, worldDataFile, worldDataFiles } from '@/api/project';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { errorMessage } from '@/lib/errors';
import { type Row, type Table, cellText, matching, parsedCell, segment, tablesOf } from './jsonTables';

type Shown = Awaited<ReturnType<typeof worldDataFile>>;

/** Most rows a table shows at once (search narrows them). */
const SHOWN_ROWS = 200;

const kb = (n: number) => `${Math.max(1, Math.round(n / 1024))} KB`;

/** One cell: its text; Edit turns it into a field, Enter or Save sets the value in the file. */
function Cell({ value, onSet }: { value: unknown; onSet: (v: unknown) => Promise<void> }) {
  const [text, setText] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  if (text === null) {
    return <button type="button" className="max-w-[28ch] truncate text-left hover:text-primary" title="Edit this value" onClick={() => setText(cellText(value))}>{cellText(value) || <span className="text-faint">—</span>}</button>;
  }
  const save = async () => { setBusy(true); try { await onSet(parsedCell(text, value)); setText(null); } finally { setBusy(false); } };
  return (
    <span className="flex items-center gap-1">
      <Input aria-label="Value" autoFocus value={text} className="h-7 min-w-[16ch] font-mono text-xs" onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => { if (e.key === 'Enter') void save(); if (e.key === 'Escape') setText(null); }} />
      <Button size="sm" disabled={busy} onClick={() => { void save(); }}>Save</Button>
    </span>
  );
}

function TableView({ table, onSet }: { table: Table; onSet: (row: Row, column: string, v: unknown) => Promise<void> }) {
  const [query, setQuery] = useState('');
  const rows = matching(table, query);
  return (
    <div className="grid min-h-0 gap-2">
      <div className="flex items-center gap-2 px-4">
        <Input type="search" aria-label={`Search ${table.title}`} placeholder="Search by id or any value" value={query} onChange={(e) => setQuery(e.target.value)} className="max-w-xs" />
        <span className="text-xs text-muted">{rows.length} of {table.rows.length} rows{rows.length > SHOWN_ROWS ? ` · showing ${SHOWN_ROWS}` : ''}</span>
      </div>
      <div className="min-h-0 overflow-auto">
        <table className="w-max min-w-full text-xs" aria-label={table.title}>
          <thead className="sticky top-0 bg-surface"><tr className="text-left text-muted"><th className="px-4 py-1 font-medium">id</th>{table.columns.map((c) => <th key={c} className="px-2 py-1 font-medium">{c}</th>)}</tr></thead>
          <tbody className="divide-y divide-line">
            {rows.slice(0, SHOWN_ROWS).map((r) => (
              <tr key={r.pointer}>
                <td className="px-4 py-1 font-mono">{r.key}</td>
                {table.columns.map((c) => <td key={c} className="px-2 py-1 font-mono"><Cell value={r.record[c]} onSet={(v) => onSet(r, c, v)} /></td>)}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

/** The file's tables, one at a time, with a value set in place written to the file. */
function Tables({ world, file, tables, onSaved }: { world: string; file: string; tables: Table[]; onSaved: () => void }) {
  const [at, setAt] = useState(0);
  const [note, setNote] = useState('');
  const set = async (row: Row, column: string, value: unknown) => {
    setNote('');
    try {
      await putWorldData(world, file, { pointer: `${row.pointer}/${segment(column)}`, value });
      setNote(`Saved ${row.key}.${column}: the next run starts from it.`);
      onSaved();
    } catch (e) { setNote(errorMessage(e)); }
  };
  const t = tables[Math.min(at, tables.length - 1)];
  return (
    <div className="grid min-h-0 flex-1 grid-rows-[auto_auto_minmax(0,1fr)] gap-2 py-3">
      {tables.length > 1 && (
        <div role="tablist" aria-label="Tables" className="flex flex-wrap gap-1 px-4">
          {tables.map((x, i) => <Button key={x.title} size="sm" variant={i === at ? 'primary' : 'default'} role="tab" aria-selected={i === at} onClick={() => setAt(i)}>{x.title} · {x.rows.length}</Button>)}
        </div>
      )}
      <p role="status" className="px-4 text-xs text-muted">{note || 'Click a value to change it.'}</p>
      <TableView key={t.title} table={t} onSet={set} />
    </div>
  );
}

function FileContents({ world, file }: { world: string; file: string }) {
  const [shown, setShown] = useState<Shown>();
  const [error, setError] = useState('');
  const [version, setVersion] = useState(0);
  useEffect(() => { worldDataFile(world, file, true).then(setShown, (e: unknown) => setError(errorMessage(e))); }, [world, file, version]);
  if (error) return <p role="alert" className="p-4 text-sm text-alarm">{error}</p>;
  if (!shown) return <p className="p-4 text-sm text-faint">Loading…</p>;
  if (shown.binary) return <p className="p-4 text-sm text-muted">Binary file · {kb(shown.size)}</p>;
  if (file.toLowerCase().endsWith('.json') && !shown.truncated) {
    let doc: unknown;
    try { doc = JSON.parse(shown.text ?? ''); } catch { doc = undefined; }
    const tables = doc === undefined ? [] : tablesOf(doc, file.split('/').pop() ?? file);
    if (tables.length) return <Tables world={world} file={file} tables={tables} onSaved={() => setVersion(version + 1)} />;
  }
  return (
    <>
      <pre className="min-h-0 flex-1 overflow-auto p-4 font-mono text-xs leading-5 whitespace-pre-wrap" aria-label={`Contents of ${file}`}>{shown.text}</pre>
      {shown.truncated && <p className="border-t border-line px-4 py-2 text-xs text-muted">Truncated: showing the first 10 MiB of {kb(shown.size)}.</p>}
    </>
  );
}

/** The files at `source` (the file itself, or those in the folder it names): one is shown, several are listed. */
function Files({ world, source }: { world: string; source: string }) {
  const [files, setFiles] = useState<string[]>();
  const [file, setFile] = useState<string>();
  const [error, setError] = useState('');
  useEffect(() => {
    const inside = (p: string) => p === source || p.startsWith(`${source.replace(/\/$/, '')}/`);
    worldDataFiles(world).then((r) => {
      const found = r.files.map((f) => f.path).filter(inside);
      setFiles(found);
      if (found.length === 1) setFile(found[0]);
    }, (e: unknown) => setError(errorMessage(e)));
  }, [world, source]);
  if (error) return <p role="alert" className="p-4 text-sm text-alarm">{error}</p>;
  if (file) return <FileContents world={world} file={file} />;
  if (!files) return <p className="p-4 text-sm text-faint">Loading…</p>;
  if (files.length === 0) return <p className="p-4 text-sm text-muted">No file here yet: upload one.</p>;
  return (
    <ul className="min-h-0 flex-1 overflow-auto p-2">
      {files.map((f) => (
        <li key={f} className="flex items-center justify-between gap-2 px-2 py-1 font-mono text-xs">
          {f}<Button size="sm" onClick={() => setFile(f)}>View</Button>
        </li>
      ))}
    </ul>
  );
}

/** A "View" button that opens `source` (a file or folder of the world's data): JSON records as tables to edit. */
export default function DataFileViewer({ world, source }: { world: string; source: string }) {
  const [open, setOpen] = useState(false);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <button type="button" className="text-xs text-primary" onClick={() => setOpen(true)}>View</button>
      <DialogContent title={source} description="The file in the world's folder. A value changed here is saved at once; the next run starts from it." className="top-[6vh] flex max-h-[88vh] w-[min(1100px,calc(100vw-24px))] flex-col">
        <Files world={world} source={source} />
      </DialogContent>
    </Dialog>
  );
}
