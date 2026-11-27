import { AlertTriangle, CornerDownRight } from 'lucide-react';
import { Link, useNavigate } from 'react-router-dom';
import { StatusMark } from '@/components/ui/status';
import { Table, Td, Th } from '@/components/ui/table';
import DeleteItemDialog from '@/features/editors/DeleteItemDialog';
import type { OrderedRow } from './list';
import { RUN_STATUS_MARK, runStatusTitle } from './runStatus';

function ProblemBadge({ row }: { row: OrderedRow }) {
  const n = row.problems.length;
  if (!n) return null;
  return (
    <span title={row.problems[0].message} className="ml-2 inline-flex items-center gap-1 rounded-full bg-warn-soft px-2 py-0.5 text-2xs font-medium text-warn">
      <AlertTriangle className="size-3" aria-hidden="true" />{n} problem{n > 1 ? 's' : ''}
    </span>
  );
}

/** Whether the experiment ran: has results, running, ran with no results, or not run yet. */
function RunStatusCell({ row }: { row: OrderedRow }) {
  if (!row.run) return <span className="text-xs text-faint">…</span>;
  return <StatusMark mark={RUN_STATUS_MARK[row.run.status]} title={runStatusTitle(row.run)} />;
}

/** The file experiments, sub-experiments indented under their parent. A click anywhere on a row opens it. */
export default function ExperimentRows({ rows, onDeleted }: { rows: OrderedRow[]; onDeleted: () => void }) {
  const navigate = useNavigate();
  return (
    <Table>
      <thead><tr><Th>Experiment</Th><Th>Status</Th><Th>Scenario</Th><Th className="text-right">Setups</Th><Th><span className="sr-only">Actions</span></Th></tr></thead>
      <tbody>
        {rows.map((r) => (
          <tr key={r.id} onClick={() => navigate(`/experiments/file/${r.id}`)} className="cursor-pointer transition-colors hover:bg-hover">
            <Td>
              <span className="inline-flex items-center" style={{ paddingLeft: r.depth * 20 }}>
                {r.depth > 0 && <CornerDownRight className="mr-1.5 size-3.5 text-faint" aria-hidden="true" />}
                <Link to={`/experiments/file/${r.id}`} className="font-medium" onClick={(e) => e.stopPropagation()}>{r.label}</Link>
                <ProblemBadge row={r} />
              </span>
            </Td>
            <Td><RunStatusCell row={r} /></Td>
            <Td className="text-muted">{r.scenario}</Td>
            <Td className="text-right tabular-nums">{r.setups}</Td>
            <Td className="w-10 text-right">
              {/* Clicks in the dialog would otherwise reach the row and open the experiment. */}
              <span onClick={(e) => e.stopPropagation()}><DeleteItemDialog compact kind="experiments" id={r.id} label={r.label} onDeleted={onDeleted} /></span>
            </Td>
          </tr>
        ))}
      </tbody>
    </Table>
  );
}
