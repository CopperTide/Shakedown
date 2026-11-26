import { useEffect, useRef, useState } from 'react';
import type { UsageBucket } from './moreChartsData';
import { bucketDate, costValue } from './moreChartsData';
import { Card } from './parts';

type Metric = 'cost_usd' | 'tokens';
const valueText = (v: number | null, metric: Metric) => metric === 'cost_usd' ? costValue(v) : (v ?? 0).toLocaleString();
const shortValue = (v: number, metric: Metric) => `${metric === 'cost_usd' ? '$' : ''}${new Intl.NumberFormat(undefined, { notation: 'compact', maximumFractionDigits: 1 }).format(v)}`;

/** Use CSS pixels for SVG geometry so chart labels do not shrink on a narrow card. */
function usePlotWidth() {
  const box = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(300);
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const measure = () => { if (el.clientWidth > 0) setWidth(Math.max(240, el.clientWidth)); };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  return { box, width };
}

/** Independent bars on a true time axis; no line suggests values in unobserved buckets. */
function UsagePlot({ rows, metric, bucket }: { rows: UsageBucket[]; metric: Metric; bucket: 'day' | 'week' }) {
  const { box, width: plotWidth } = usePlotWidth();
  const plotEnd = plotWidth - 16, plotSpan = plotEnd - 54;
  const dates = rows.map((r) => Date.parse(r.t));
  const bucketMs = (bucket === 'week' ? 7 : 1) * 86_400_000;
  const first = dates[0] - bucketMs / 2, last = dates[dates.length - 1] + bucketMs / 2;
  const max = Math.max(0, ...rows.map((r) => r[metric] ?? 0)) || 1;
  const x = (t: number) => 54 + (t - first) / (last - first) * plotSpan;
  const y = (v: number) => 178 - v / max * 150;
  const width = Math.min(36, Math.max(1, bucketMs / (last - first) * plotSpan * 0.7));
  const tickIndices = [...new Set(plotWidth < 420 ? [0, dates.length - 1] : [0, Math.floor((dates.length - 1) / 2), dates.length - 1])];
  return <div ref={box} className="w-full overflow-x-auto"><svg viewBox={`0 0 ${plotWidth} 220`} width={plotWidth} height={220} className="block" role="img" aria-label={`${metric === 'cost_usd' ? 'Known cost in USD' : 'Tokens'} per ${bucket}, UTC. Exact values in the data table below.`}>
    {[0, max / 2, max].map((v) => <g key={v}><line x1="54" x2={plotEnd} y1={y(v)} y2={y(v)} stroke="var(--line)" /><text x="46" y={y(v) + 4} textAnchor="end" fill="var(--muted)" fontSize="11">{shortValue(v, metric)}</text></g>)}
    {rows.map((r, i) => r[metric] === null ? <text key={r.t} x={x(dates[i])} y="174" textAnchor="middle" fill="var(--muted)" fontSize="13">×<title>{bucketDate(r.t)}: cost unknown</title></text> : <g key={r.t}><rect x={x(dates[i]) - width / 2} y={y(r[metric]!)} width={width} height={178 - y(r[metric]!)} fill="var(--accent)" /><circle cx={x(dates[i])} cy={y(r[metric]!)} r="2.5" fill="var(--accent)" /><title>{bucketDate(r.t)}: {valueText(r[metric], metric)}</title></g>)}
    {tickIndices.map((i) => <text key={i} x={x(dates[i])} y="202" textAnchor={i === 0 ? 'start' : i === dates.length - 1 ? 'end' : 'middle'} fill="var(--muted)" fontSize="11">{new Date(dates[i]).toLocaleDateString(undefined, { month: 'short', day: 'numeric', timeZone: 'UTC' })}</text>)}
  </svg></div>;
}

function UsageTable({ rows, metric, bucket }: { rows: UsageBucket[]; metric: Metric; bucket: 'day' | 'week' }) {
  return <details className="mt-2 border-t border-line pt-2"><summary className="cursor-pointer text-xs text-muted">View {metric === 'cost_usd' ? 'cost' : 'token'} data</summary>
    <div className="max-h-72 overflow-auto" tabIndex={0}><table className="mt-2 w-full text-left text-xs" aria-label={`${metric === 'cost_usd' ? 'Cost' : 'Token'} data by ${bucket}`}><thead><tr><th scope="col" className="py-2">{bucket === 'week' ? 'Week of' : 'Date'} (UTC)</th><th scope="col" className="text-right">{metric === 'cost_usd' ? 'Known USD' : 'Tokens'}</th><th scope="col" className="text-right">Runs</th>{metric === 'cost_usd' && <th scope="col" className="text-right">Unknown cost</th>}</tr></thead>
      <tbody>{rows.map((r) => <tr key={r.t} className="border-t border-line"><th scope="row" className="py-2 font-normal">{bucketDate(r.t)}</th><td className="text-right font-mono">{valueText(r[metric], metric)}</td><td className="text-right">{r.runs.toLocaleString()}</td>{metric === 'cost_usd' && <td className="text-right">{r.uncosted.toLocaleString()}</td>}</tr>)}</tbody>
    </table></div>
  </details>;
}

export default function UsageChart({ rows, metric, bucket }: { rows: UsageBucket[]; metric: Metric; bucket: 'day' | 'week' }) {
  const unknown = rows.reduce((n, r) => n + r.uncosted, 0);
  const runs = rows.reduce((n, r) => n + r.runs, 0);
  const known = rows.some((r) => r[metric] !== null);
  const total = rows.reduce((n, r) => n + (r[metric] ?? 0), 0);
  return <Card title={metric === 'cost_usd' ? 'Cost over time' : 'Tokens over time'} hint={`Totals per ${bucket} (UTC), across all filtered setups/models, by run creation date.`}>
    {!rows.length ? <p className="py-6 text-sm text-muted">No finished runs in this time range.</p> : <>
      <p className="mb-1 text-sm">{metric === 'cost_usd' ? 'Known cost: ' : 'Tokens: '}<strong className="font-mono">{known ? valueText(total, metric) : 'Unknown'}</strong> · {runs.toLocaleString()} runs</p>
      {known ? <UsagePlot rows={rows} metric={metric} bucket={bucket} /> : <p className="py-8 text-sm text-muted">No known costs to plot for these runs.</p>}
      {metric === 'cost_usd' && unknown > 0 && <p className="text-xs text-muted">Cost unavailable for {unknown.toLocaleString()} {unknown === 1 ? 'run' : 'runs'}; these are excluded from totals. × marks a bucket with no known cost.</p>}
      <UsageTable rows={rows} metric={metric} bucket={bucket} />
    </>}
  </Card>;
}
