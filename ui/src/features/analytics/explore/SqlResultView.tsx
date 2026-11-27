// An SQL query's answer: a chart of picked columns (or none), the rows, and a CSV of every row.
import { Download } from 'lucide-react';
import { useMemo, useState } from 'react';
import type { Chart, SqlResult } from '@/api/explore';
import { Button } from '@/components/ui/button';
import { NativeSelect } from '@/components/ui/input';
import { Segmented } from '@/components/ui/segmented';
import { Table, Td, Th, Tr } from '@/components/ui/table';
import { cn } from '@/lib/cn';
import { defaultAxes, sqlChart } from './chartData';
import ExploreChart from './ExploreChart';
import MultiPicker from './MultiPicker';
import { CHARTS } from './spec';
import { download, sqlText, toCsv } from './values';

/** The most rows the table draws; the CSV has them all. */
export const SHOWN = 500;

export interface Axes { type: Chart; x: string; ys: string[] }

function Rows({ r }: { r: SqlResult }) {
  return (
    <Table wrapperClassName="max-h-[28rem] overflow-y-auto" aria-label="SQL result">
      <thead className="sticky top-0"><tr>{r.columns.map((c, i) => <Th key={i} title={c.type} className={cn(c.kind === 'number' && 'text-right')}>{c.name}</Th>)}</tr></thead>
      <tbody>
        {r.rows.slice(0, SHOWN).map((row, ri) => (
          <Tr key={ri}>
            {row.map((v, ci) => (
              <Td key={ci} className={cn('max-w-80 truncate', r.columns[ci]?.kind === 'number' && 'text-right font-mono tabular-nums', v == null && 'text-faint')} title={sqlText(v)}>
                {v == null ? 'null' : sqlText(v)}
              </Td>
            ))}
          </Tr>
        ))}
      </tbody>
    </Table>
  );
}

function ChartPicks({ r, axes, onAxes }: { r: SqlResult; axes: Axes; onAxes: (a: Axes) => void }) {
  const numeric = r.columns.filter((c) => c.kind === 'number').map((c) => ({ id: c.name, label: c.name }));
  return (
    <div className="flex flex-wrap items-center gap-2 text-xs text-muted">
      <label className="flex items-center gap-1.5">X
        <NativeSelect aria-label="X column" value={axes.x} onChange={(e) => onAxes({ ...axes, x: e.target.value })} className="h-7 w-40 text-xs">
          {r.columns.map((c) => <option key={c.name} value={c.name}>{c.name}</option>)}
        </NativeSelect>
      </label>
      <MultiPicker label="Y columns" options={numeric} picked={axes.ys} onChange={(ys) => onAxes({ ...axes, ys })} summary={axes.ys.length ? `Y: ${axes.ys.join(', ')}` : 'Y: pick numbers'} />
    </div>
  );
}

export default function SqlResultView({ r, axes: initial, onAxes }: { r: SqlResult; axes?: Axes; onAxes?: (a: Axes) => void }) {
  const [axes, setAxesState] = useState<Axes>(() => initial ?? { type: 'table', ...defaultAxes(r) });
  const setAxes = (a: Axes) => { setAxesState(a); onAxes?.(a); };
  const data = useMemo(() => sqlChart(r, axes.x, axes.ys), [r, axes.x, axes.ys]);
  const csv = () => download('explore-sql.csv', toCsv(r.columns.map((c) => c.name), r.rows));
  return (
    <div className="flex flex-col gap-3 p-4">
      <div className="flex flex-wrap items-center gap-2">
        <Segmented label="Show as" value={axes.type} options={CHARTS} onChange={(type) => setAxes({ ...axes, type })} />
        {axes.type !== 'table' && <ChartPicks r={r} axes={axes} onAxes={setAxes} />}
        <Button size="sm" className="ml-auto" onClick={csv}><Download />CSV</Button>
      </div>
      {axes.type !== 'table' && (axes.ys.length
        ? <ExploreChart data={data} kind={axes.type} label={`${axes.ys.join(', ')} by ${axes.x}`} />
        : <p className="text-xs text-muted">Pick at least one numeric column to chart.</p>)}
      <Rows r={r} />
      <p role="status" className="text-xs text-faint">
        {r.rows.length.toLocaleString()} row{r.rows.length === 1 ? '' : 's'}
        {r.rows.length > SHOWN && `, first ${SHOWN} shown (the CSV has all)`}
        {r.truncated && ` · cut at the ${r.truncated === 'rows' ? 'row' : 'size'} limit: add a LIMIT or aggregate`} · {r.ms.toLocaleString()} ms
      </p>
    </div>
  );
}
