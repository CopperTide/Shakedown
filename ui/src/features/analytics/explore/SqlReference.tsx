// What an SQL query may read: the documented views and their columns (a click puts the name in the
// query), the functions it may call, the limits it runs under, and example queries.
import type { ExploreSchema, SqlView } from '@/api/explore';

function View({ v, onInsert }: { v: SqlView; onInsert: (text: string) => void }) {
  return (
    <details className="border-b border-line last:border-b-0">
      <summary className="flex cursor-pointer items-baseline gap-2 px-3 py-1.5 text-xs hover:bg-hover">
        <button type="button" onClick={(e) => { e.preventDefault(); onInsert(v.name); }} className="font-mono font-semibold hover:text-accent" title="Put the view's name in the query">{v.name}</button>
        <span className="text-faint">{v.columns.length} columns</span>
      </summary>
      <p className="px-3 pb-1 text-2xs text-muted">{v.description}</p>
      <ul className="pb-2">
        {v.columns.map((c) => (
          <li key={c.name}>
            <button type="button" onClick={() => onInsert(c.name)} title={c.description}
              className="flex w-full items-baseline gap-2 px-3 py-0.5 text-left text-2xs hover:bg-hover">
              <span className="font-mono text-fg">{c.name}</span><span className="text-faint">{c.type}</span>
            </button>
          </li>
        ))}
      </ul>
    </details>
  );
}

export default function SqlReference({ sql, onInsert, onExample }: { sql: ExploreSchema['sql']; onInsert: (text: string) => void; onExample: (sql: string) => void }) {
  const l = sql.limits;
  return (
    <aside aria-label="SQL reference" className="flex min-w-0 flex-col gap-3 text-xs">
      <section className="overflow-hidden rounded-md border border-line bg-surface">
        <h3 className="border-b border-line px-3 py-1.5 text-2xs font-medium text-muted">Views</h3>
        {sql.views.map((v) => <View key={v.name} v={v} onInsert={onInsert} />)}
      </section>
      {sql.examples.length > 0 && (
        <section className="flex flex-col gap-1">
          <h3 className="text-2xs font-medium text-muted">Examples</h3>
          {sql.examples.map((e) => (
            <button key={e.name} type="button" onClick={() => onExample(e.sql)} className="truncate text-left text-accent hover:underline" title={e.sql}>{e.name}</button>
          ))}
        </section>
      )}
      <p className="text-2xs text-muted">
        One SELECT (or WITH) over these views, read-only: at most {l.rows.toLocaleString()} rows, {l.columns} columns and {l.timeout_seconds} s.
      </p>
      <details className="text-2xs text-muted">
        <summary className="cursor-pointer">Functions you can call ({sql.functions.length})</summary>
        <p className="mt-1 break-words font-mono">{sql.functions.join(', ')}</p>
      </details>
    </aside>
  );
}
