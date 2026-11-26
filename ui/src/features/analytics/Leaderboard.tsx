import type { ColumnDef } from '@tanstack/react-table';
import type { BoardRow } from '@/api/analytics';
import { DataTable } from '@/components/ui/data-table';
import { EmptyState } from '@/components/ui/state';
import { duration } from '@/lib/format';
import { money, pct } from './parts';

const num = 'w-24 text-right tabular-nums';
/** A missing figure sorts last whichever way the column sorts. */
const orNull = (x: number | null) => x ?? undefined;

const COLUMNS: ColumnDef<BoardRow, unknown>[] = [
  {
    id: 'setup', header: 'Setup', accessorKey: 'setup', meta: { className: 'max-w-0 w-[34%]' },
    cell: ({ row }) => <span className="block truncate" title={row.original.setup}>{row.original.setup}</span>,
  },
  {
    id: 'scenario', header: 'Scenario', accessorFn: (r) => `${r.scenario} ${r.world}`, meta: { className: 'max-w-0 w-[26%]' },
    cell: ({ row }) => (
      <span className="block truncate" title={`${row.original.scenario} · ${row.original.world || 'no world named'}`}>
        {row.original.scenario}{row.original.world && <span className="text-faint"> · {row.original.world}</span>}
      </span>
    ),
  },
  { id: 'runs', header: 'Runs', accessorKey: 'runs', meta: { className: num } },
  {
    id: 'pass_rate', header: 'Pass rate', accessorFn: (r) => orNull(r.pass_rate), sortUndefined: 'last', meta: { className: num },
    cell: ({ row }) => <span title={`${row.original.passed} of ${row.original.judged} judged`}>{pct(row.original.pass_rate)}</span>,
  },
  { id: 'cost_per_run', header: 'Cost / run', accessorFn: (r) => orNull(r.cost_per_run), sortUndefined: 'last', meta: { className: num }, cell: ({ row }) => money(row.original.cost_per_run) },
  { id: 'time', header: 'Median time', accessorFn: (r) => orNull(r.median_seconds), sortUndefined: 'last', meta: { className: num }, cell: ({ row }) => duration(row.original.median_seconds) },
  { id: 'cost_per_pass', header: 'Cost / pass', accessorFn: (r) => orNull(r.cost_per_pass), sortUndefined: 'last', meta: { className: num }, cell: ({ row }) => money(row.original.cost_per_pass) },
];

/** Each setup on each scenario: runs, pass rate, cost and time, sortable; a row opens its runs. */
export default function Leaderboard({ rows, onOpen }: { rows: BoardRow[]; onOpen: (r: BoardRow) => void }) {
  return (
    <DataTable label="Leaderboard" columns={COLUMNS} data={rows} getRowId={(r) => `${r.setup}\n${r.world}\n${r.scenario}`}
      sorting={[{ id: 'pass_rate', desc: true }]} loadingLabel="Loading the leaderboard…" onOpen={onOpen}
      empty={<EmptyState title="No runs to rank" />} />
  );
}
