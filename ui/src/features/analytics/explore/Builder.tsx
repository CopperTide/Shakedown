// The builder: measures split by up to two dimensions over the runs a filter picks, drawn as a
// chart above the exact table. Every bar and every value opens the runs or steps behind it.
import { Download } from 'lucide-react';
import { useMemo, useState } from 'react';
import { type ExploreSchema, exploreQuery, type QueryResult, type Spec } from '@/api/explore';
import { Button } from '@/components/ui/button';
import { Panel } from '@/components/ui/panel';
import { Segmented } from '@/components/ui/segmented';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/state';
import { NativeSelect } from '@/components/ui/input';
import { useQuery } from '@/hooks/useQuery';
import BuilderControls from './BuilderControls';
import { builderChart } from './chartData';
import DrillDialog from './DrillDialog';
import ExploreChart from './ExploreChart';
import ResultTable, { type DrillAt, drillAt } from './ResultTable';
import SaveView from './SaveView';
import { apiSpec, type BuilderState, CHARTS, chartedMeasure, savedSpec } from './spec';
import { dimensionText, download, measureText, toCsv } from './values';

/** The result as CSV: labels as headers, experiments by name, raw numbers. */
function csvOf(r: QueryResult): string {
  return toCsv(r.columns.map((c) => c.label), r.rows.map((row) => row.map((v, i) => (r.columns[i].role === 'dimension' ? dimensionText(v, r.columns[i].id, r.labels) : v))));
}

function ChartBox({ s, r, onChange, onDrill }: { s: BuilderState; r: QueryResult; onChange: (s: BuilderState) => void; onDrill: (d: DrillAt) => void }) {
  const y = chartedMeasure(s);
  const data = useMemo(() => builderChart(r, y), [r, y]);
  const measures = r.columns.filter((c) => c.role === 'measure');
  const pick = (xi: number, si: number) => {
    const row = r.rows.find((row) => JSON.stringify(row[0]) === JSON.stringify(data.keys[xi]) && (data.series.length < 2 || JSON.stringify(row[1]) === JSON.stringify(data.series[si].key)));
    if (row) onDrill(drillAt(r, row, r.columns.findIndex((c) => c.id === y)));
  };
  return (
    <div className="flex flex-col gap-2">
      {measures.length > 1 && (
        <label className="flex items-center gap-1.5 self-start text-xs text-muted">Chart
          <NativeSelect aria-label="Measure charted" value={y} onChange={(e) => onChange({ ...s, y: e.target.value as BuilderState['y'] })} className="h-7 w-40 text-xs">
            {measures.map((m) => <option key={m.id} value={m.id}>{m.label}</option>)}
          </NativeSelect>
        </label>
      )}
      <ExploreChart data={data} kind={s.chart === 'table' ? 'bar' : s.chart} onPick={pick}
        label={`${measures.find((m) => m.id === y)?.label ?? y}${data.labels.length ? `, ${data.labels.length} groups` : ''}`} />
    </div>
  );
}

function Result({ s, r, onChange, onDrill }: { s: BuilderState; r: QueryResult; onChange: (s: BuilderState) => void; onDrill: (d: DrillAt) => void }) {
  if (!r.rows.length) return <EmptyState title="No finished runs match" hint="Pick a longer time range or remove a filter. Runs count once they end and their figures are worked out." />;
  const total = r.rows.length === 1 && r.columns[0].role === 'measure';
  return (
    <div className="flex flex-col gap-3 p-4">
      {s.chart !== 'table' && !total && <ChartBox s={s} r={r} onChange={onChange} onDrill={onDrill} />}
      <ResultTable result={r} onDrill={onDrill} />
      <p className="text-xs text-faint">
        {r.rows.length.toLocaleString()} row{r.rows.length === 1 ? '' : 's'}{r.truncated ? ' (more groups left out: narrow the filter)' : ''} · {measureText(r.ms, 'count')} ms · click any value for the runs or steps behind it
      </p>
    </div>
  );
}

export default function Builder({ s, schema, onChange }: { s: BuilderState; schema: ExploreSchema; onChange: (s: BuilderState) => void }) {
  const [now] = useState(() => new Date());
  const spec: Spec = useMemo(() => apiSpec(s, now), [s, now]);
  const key = JSON.stringify(spec);
  const q = useQuery(`explore:query:${key}`, () => exploreQuery(spec), { keepPrevious: true });
  const [drill, setDrill] = useState<DrillAt | null>(null);
  const label = (id: string) => schema.measures.find((m) => m.id === id)?.label ?? schema.dimensions.find((d) => d.id === id)?.label ?? id;
  const name = `${label(chartedMeasure(s))}${s.dimensions.length ? ` by ${s.dimensions.map(label).join(' and ').toLowerCase()}` : ''}`;
  return (
    <div className="flex flex-col gap-3">
      <BuilderControls s={s} schema={schema} onChange={onChange} />
      <Panel>
        <header className="flex flex-wrap items-center gap-2 border-b border-line px-4 py-2">
          <h2 className="min-w-0 flex-1 truncate font-display text-lg">{name}</h2>
          <Segmented label="Chart" value={s.chart} options={CHARTS} onChange={(chart) => onChange({ ...s, chart })} />
          <Button size="sm" disabled={!q.data} onClick={() => q.data && download('explore.csv', csvOf(q.data))}><Download />CSV</Button>
          <SaveView suggest={name} view={() => ({ kind: 'builder', spec: savedSpec(s) })} />
        </header>
        {q.error && <ErrorState error={q.error} onRetry={q.reload} className="m-4" />}
        {q.data ? <Result s={s} r={q.data} onChange={onChange} onDrill={setDrill} /> : !q.error && <LoadingState label="Working it out…" rows={4} className="m-4" />}
      </Panel>
      <DrillDialog spec={spec} at={drill} onClose={() => setDrill(null)} />
    </div>
  );
}
