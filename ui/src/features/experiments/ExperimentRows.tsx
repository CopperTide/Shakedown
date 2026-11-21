import { AlertTriangle, CornerDownRight } from 'lucide-react';
import { Link } from 'react-router-dom';
import { Table, Td, Th } from '@/components/ui/table';
import type { OrderedRow } from './list';

function ProblemBadge({ row }: { row: OrderedRow }) {
  const n = row.problems.length;
  if (!n) return null;
  return (
    <span title={row.problems[0].message} className="ml-2 inline-flex items-center gap-1 rounded-full bg-warn-soft px-2 py-0.5 text-2xs font-medium text-warn">
      <AlertTriangle className="size-3" aria-hidden="true" />{n} problem{n > 1 ? 's' : ''}
    </span>
  );
}

/** The file experiments, sub-experiments indented under their parent. */
export default function ExperimentRows({ rows }: { rows: OrderedRow[] }) {
  return (
    <Table>
      <thead><tr><Th>Experiment</Th><Th>Scenario</Th><Th className="text-right">Setups</Th></tr></thead>
      <tbody>
        {rows.map((r) => (
          <tr key={r.id}>
            <Td>
              <span className="inline-flex items-center" style={{ paddingLeft: r.depth * 20 }}>
                {r.depth > 0 && <CornerDownRight className="mr-1.5 size-3.5 text-faint" aria-hidden="true" />}
                <Link to={`/experiments/file/${r.id}`} className="font-medium hover:text-accent">{r.label}</Link>
                <ProblemBadge row={r} />
              </span>
            </Td>
            <Td className="text-muted">{r.scenario}</Td>
            <Td className="text-right tabular-nums">{r.setups}</Td>
          </tr>
        ))}
      </tbody>
    </Table>
  );
}
