// The Analytics page's Explore tab: a measures × dimensions builder (the Looker-like part), SQL
// over the documented views for power users, and the views anyone saved. The mode, the builder's
// query and the last SQL query live in the address, so a link reopens what is on screen.
//
//   ?tab=explore&m=cost&m=runs&d=agent     builder (spec.ts)
//   ?tab=explore&mode=sql&sql=SELECT…      SQL tab
//   ?tab=explore&mode=saved                saved views
import { useCallback, useMemo } from 'react';
import { useSearchParams } from 'react-router-dom';
import { exploreSchema, type SavedView } from '@/api/explore';
import { Panel } from '@/components/ui/panel';
import { Segmented } from '@/components/ui/segmented';
import { ErrorState, LoadingState } from '@/components/ui/state';
import { useQuery } from '@/hooks/useQuery';
import Builder from './Builder';
import SavedViews from './SavedViews';
import { type BuilderState, fromSaved, parseBuilder, writeBuilder } from './spec';
import SqlTab from './SqlTab';

type Mode = 'builder' | 'sql' | 'saved';
const MODES: readonly { id: Mode; label: string }[] = [{ id: 'builder', label: 'Builder' }, { id: 'sql', label: 'SQL' }, { id: 'saved', label: 'Saved views' }];

/** The mode, the builder's state and the SQL query, kept in the address. */
function useExplore() {
  const [sp, setSp] = useSearchParams();
  const mode: Mode = MODES.some((m) => m.id === sp.get('mode')) ? (sp.get('mode') as Mode) : 'builder';
  const key = sp.toString();
  const builder = useMemo(() => parseBuilder(sp), [key]); // eslint-disable-line react-hooks/exhaustive-deps
  const edit = useCallback((fn: (p: URLSearchParams) => URLSearchParams) => setSp((prev) => fn(new URLSearchParams(prev)), { replace: true }), [setSp]);
  const setMode = (m: Mode) => edit((p) => { if (m === 'builder') p.delete('mode'); else p.set('mode', m); return p; });
  const setBuilder = (s: BuilderState) => edit((p) => writeBuilder(p, s));
  const setSql = (sql: string) => edit((p) => { p.set('sql', sql); return p; });
  const open = (v: SavedView) => edit((p) => {
    if (v.kind === 'sql') { p.set('mode', 'sql'); p.set('sql', v.spec.sql); return p; }
    p.delete('mode');
    return writeBuilder(p, fromSaved(v.spec));
  });
  return { mode, setMode, builder, setBuilder, sql: sp.get('sql'), setSql, open };
}

export default function ExploreTab() {
  const x = useExplore();
  const schema = useQuery('explore:schema', exploreSchema, { shared: true });
  const s = schema.data;
  return (
    <div className="flex flex-col gap-4">
      <Segmented label="Explore with" value={x.mode} options={MODES} onChange={x.setMode} />
      {!s && (schema.error ? <ErrorState error={schema.error} onRetry={schema.retry} /> : <Panel><LoadingState label="Loading Explore…" rows={3} /></Panel>)}
      {s && x.mode === 'builder' && <Builder s={x.builder} schema={s} onChange={x.setBuilder} />}
      {s && x.mode === 'sql' && <SqlTab key={x.sql ?? ''} schema={s} sql={x.sql} onRan={x.setSql} />}
      {x.mode === 'saved' && <SavedViews onOpen={x.open} />}
    </div>
  );
}
