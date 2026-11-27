// The builder's result as a table: dimension values, then each measure; every value opens the runs
// or steps it is made of.
import type { Cell, DimensionId, MeasureId, QueryResult } from '@/api/explore';
import { Table, Td, Th, Tr } from '@/components/ui/table';
import { cn } from '@/lib/cn';
import { dimensionText, measureText } from './values';

/** What a click asks for: the row's dimension values, and the measure clicked (none: the row's runs). */
export interface DrillAt { cell: Partial<Record<DimensionId, string | null>>; measure?: MeasureId; title: string }

/** The drill of row `row` of `r`, at measure column `col` (or none). */
export function drillAt(r: QueryResult, row: Cell[], col: number | null): DrillAt {
  const cell: DrillAt['cell'] = {};
  const parts: string[] = [];
  r.columns.forEach((c, i) => {
    if (c.role !== 'dimension') return;
    cell[c.id as DimensionId] = row[i] == null ? null : String(row[i]);
    parts.push(dimensionText(row[i], c.id, r.labels));
  });
  const m = col == null ? undefined : r.columns[col];
  return { cell, measure: m?.id as MeasureId | undefined, title: [m?.label ?? 'Runs', ...parts].join(' · ') };
}

export default function ResultTable({ result, onDrill }: { result: QueryResult; onDrill: (d: DrillAt) => void }) {
  const r = result;
  return (
    <Table wrapperClassName="max-h-[28rem] overflow-y-auto" aria-label="Explore result">
      <thead className="sticky top-0">
        <tr>{r.columns.map((c) => <Th key={c.id} className={cn(c.role === 'measure' && 'text-right')}>{c.label}</Th>)}</tr>
      </thead>
      <tbody>
        {r.rows.map((row, ri) => (
          <Tr key={ri}>
            {row.map((v, ci) => {
              const c = r.columns[ci];
              const dim = c.role === 'dimension';
              const text = dim ? dimensionText(v, c.id, r.labels) : measureText(v, c.unit);
              return (
                <Td key={ci} className={cn(!dim && 'text-right font-mono tabular-nums', dim && v == null && 'text-faint')}>
                  <button type="button" onClick={() => onDrill(drillAt(r, row, dim ? null : ci))} title={dim ? 'Open these runs' : `Open what makes up this ${c.label.toLowerCase()}`}
                    className="max-w-72 truncate rounded-sm text-left hover:text-accent hover:underline focus-visible:text-accent">{text}</button>
                </Td>
              );
            })}
          </Tr>
        ))}
      </tbody>
    </Table>
  );
}
