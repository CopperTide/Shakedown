import type { FailureCount } from '@/api/analytics';

/** The table is the chart: names and exact counts stay visible without hovering or color. */
export default function CountBars({ rows, label, empty, ordered = false }: { rows: FailureCount[]; label: string; empty: string; ordered?: boolean }) {
  const data = ordered ? rows : [...rows].sort((a, b) => b.runs - a.runs || a.name.localeCompare(b.name));
  const max = Math.max(1, ...data.map((r) => r.runs));
  if (!data.length || data.every((r) => r.runs === 0)) return <p className="py-6 text-sm text-muted">{empty}</p>;
  return <div className="max-h-80 overflow-auto" tabIndex={0}><table className="w-full table-fixed text-xs" aria-label={label}>
    <thead><tr className="text-left text-muted"><th scope="col" className="w-3/4 py-2 font-normal">{ordered ? 'Run time' : 'Name'}</th><th scope="col" className="py-2 text-right font-normal">Runs</th></tr></thead>
    <tbody>{data.map((r) => <tr key={r.name} className="border-t border-line align-top">
      <th scope="row" className="break-words py-2 pr-3 text-left font-normal"><span>{r.name}</span><span aria-hidden="true" className="mt-1.5 block h-1.5 rounded-sm bg-accent" style={{ width: `${r.runs / max * 100}%` }} /></th>
      <td className="py-2 text-right font-mono tabular-nums">{r.runs.toLocaleString()}</td>
    </tr>)}</tbody>
  </table></div>;
}
