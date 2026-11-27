// The SQL tab, for power users: one read-only SELECT over the documented views, run with
// Ctrl/Cmd+Enter, answered as a table (or a chart) with a CSV of every row. The query that last
// ran is kept in the address, so a link reruns it (the parent remounts this tab per query).
import { Play } from 'lucide-react';
import { useRef, useState } from 'react';
import { type ExploreSchema, exploreSql, type SqlResult } from '@/api/explore';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/input';
import { Panel } from '@/components/ui/panel';
import { LoadingState } from '@/components/ui/state';
import { useQuery } from '@/hooks/useQuery';
import SaveView from './SaveView';
import SqlReference from './SqlReference';
import SqlResultView, { type Axes } from './SqlResultView';

export const STARTER = 'SELECT setup, count(*) AS runs, avg(cost_usd) AS avg_cost\n  FROM runs\n GROUP BY setup\n ORDER BY runs DESC';

/** The editor, with the text put at the cursor when a name in the reference is clicked. */
function useEditor(initial: string) {
  const ref = useRef<HTMLTextAreaElement>(null);
  const [text, setText] = useState(initial);
  const insert = (s: string) => {
    const el = ref.current;
    const at = el?.selectionStart ?? text.length, end = el?.selectionEnd ?? text.length;
    setText(text.slice(0, at) + s + text.slice(end));
    requestAnimationFrame(() => { el?.focus(); el?.setSelectionRange(at + s.length, at + s.length); });
  };
  return { ref, text, setText, insert };
}

function Answer({ ran, sqlKey }: { ran: { sql: string; n: number } | null; sqlKey: string | null }) {
  const [axes, setAxes] = useState<Axes | undefined>(undefined);
  const q = useQuery<SqlResult>(sqlKey, () => exploreSql(ran!.sql), { keepPrevious: true });
  if (!ran) return <p className="p-4 text-sm text-muted">Write a query and press Run (Ctrl+Enter). The views are listed on the right.</p>;
  return (
    <>
      {q.error && <p role="alert" className="m-4 whitespace-pre-wrap rounded-md border border-alarm/40 bg-alarm-soft px-3 py-2 font-mono text-xs text-alarm">{q.error}</p>}
      {q.data && !q.error && <SqlResultView key={q.data.sql} r={q.data} axes={axes} onAxes={setAxes} />}
      {!q.data && !q.error && <LoadingState label="Running…" rows={3} className="m-4" />}
    </>
  );
}

export default function SqlTab({ schema, sql, onRan }: { schema: ExploreSchema; sql: string | null; onRan: (sql: string) => void }) {
  const ed = useEditor(sql ?? STARTER);
  const [ran, setRan] = useState<{ sql: string; n: number } | null>(sql ? { sql, n: 0 } : null);
  const run = () => {
    if (!ed.text.trim()) return;
    // A new query goes into the address (the tab opens again on it and runs it); the same one runs again here.
    if (ed.text === sql) setRan({ sql, n: (ran?.n ?? 0) + 1 });
    else onRan(ed.text);
  };
  return (
    <div className="grid grid-cols-1 gap-4 lg:grid-cols-[minmax(0,1fr)_16rem]">
      <div className="flex min-w-0 flex-col gap-3">
        <Panel className="flex flex-col">
          <Textarea ref={ed.ref} aria-label="SQL query" spellCheck={false} rows={8} value={ed.text} onChange={(e) => ed.setText(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) { e.preventDefault(); run(); } }}
            className="resize-y rounded-b-none border-0 border-b border-line bg-surface p-3 text-sm" />
          <div className="flex flex-wrap items-center gap-2 px-3 py-2">
            <Button size="sm" variant="primary" onClick={run} disabled={!ed.text.trim()}><Play />Run</Button>
            <span className="text-2xs text-faint">Ctrl/⌘ + Enter</span>
            <span className="ml-auto" />
            <SaveView suggest="My query" view={() => ({ kind: 'sql', spec: { sql: ed.text } })} />
          </div>
        </Panel>
        <Panel><Answer ran={ran} sqlKey={ran ? `explore:sql:${ran.n}:${ran.sql}` : null} /></Panel>
      </div>
      <SqlReference sql={schema.sql} onInsert={ed.insert} onExample={(s) => ed.setText(s)} />
    </div>
  );
}
