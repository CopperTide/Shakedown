import type { ColumnDef } from '@tanstack/react-table';
import { MoreHorizontal, Play } from 'lucide-react';
import { useCallback, useMemo, useState } from 'react';
import { Link, useNavigate, useSearchParams } from 'react-router-dom';
import { toast } from 'sonner';
import { deleteRun, runs as fetchRuns } from '@/api/runs';
import { ApproximateMark, StatusChip } from '@/components/marks';
import { Button } from '@/components/ui/button';
import { DataTable } from '@/components/ui/data-table';
import { Dialog, DialogActions, DialogContent } from '@/components/ui/dialog';
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '@/components/ui/dropdown-menu';
import { PageBody, PageHeader } from '@/components/ui/page';
import { Panel } from '@/components/ui/panel';
import { Hint } from '@/components/ui/hint';
import { EmptyState, ErrorState, Notice } from '@/components/ui/state';
import { useAction } from '@/hooks/useAction';
import { DISK_CHANGED } from '@/hooks/useHealth';
import { pollWhile, useQuery } from '@/hooks/useQuery';
import { useTaskNames } from '@/hooks/useTasks';
import { ago, formatTime } from '@/lib/time';
import { qs } from '@/lib/url';
import type { RunSummary } from '@/model/types';
import { describeRunFilter, filterRuns, isFiltered, parseRunFilter } from './runFilter';
import { type BranchRow, branchRows, runIsRunning, runTitle } from './runsTable';

/** The runs, read again every 3 s while any of them runs. */
function useRuns() {
  const { data: runs, error, reload: refresh } = useQuery('runs', fetchRuns, pollWhile(3000, (r) => r.some(runIsRunning)));
  return { runs, error, refresh };
}

function RowMenu({ row, onDelete }: { row: BranchRow; onDelete: (r: RunSummary) => void }) {
  const nav = useNavigate();
  const running = runIsRunning(row.run);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="icon" aria-label="Run actions" className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100 data-[state=open]:opacity-100" onClick={(e) => e.stopPropagation()}><MoreHorizontal /></Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent onClick={(e) => e.stopPropagation()}>
        <DropdownMenuItem onSelect={() => nav(openOf(row))}>Open</DropdownMenuItem>
        <DropdownMenuItem onSelect={() => nav(`/runs/${row.run.id}/${row.branch.id}`)}>All details</DropdownMenuItem>
        <DropdownMenuItem onSelect={() => nav(`/control/${row.run.id}/${row.branch.id}`)}>Process view</DropdownMenuItem>
        <DropdownMenuItem disabled={running} className="text-alarm" onSelect={() => onDelete(row.run)}>
          {running ? 'Delete run (it is still running)' : 'Delete run…'}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** A row opens on the Run screen: its result and timeline, with everything else one click away. */
const openOf = (r: BranchRow) => `/run${qs({ run: r.run.id, branch: r.branch.id })}`;

/** The run's name (its team is the tooltip); a rerun is "↳ Rerun", its note the tooltip. */
function NameCell({ row, name }: { row: BranchRow; name: string }) {
  const team = row.branch.team.name;
  if (row.depth === 0) return <span className="block truncate" title={`${name} · ${team}`}>{name}</span>;
  return (
    <span className="flex min-w-0 items-center gap-1.5 text-muted" style={{ paddingLeft: (row.depth - 1) * 14 }} title={[row.branch.fork_note, team].filter(Boolean).join(' · ')}>
      <span aria-hidden="true" className="font-mono">↳</span><span>Rerun</span>
      <ApproximateMark of={row.branch} />
    </span>
  );
}

function columns(nameOf: (r: BranchRow) => string, onDelete: (r: RunSummary) => void): ColumnDef<BranchRow, unknown>[] {
  return [
    { id: 'name', header: 'Name', enableSorting: false, cell: ({ row }) => <NameCell row={row.original} name={nameOf(row.original)} />, meta: { className: 'w-full max-w-0' } },
    { id: 'team', header: 'Team', enableSorting: false, cell: ({ row }) => <span className="block max-w-48 truncate text-xs text-muted">{row.original.branch.team.name}</span>, meta: { className: 'hidden md:table-cell' } },
    { id: 'result', header: 'Result', accessorFn: (r) => r.branch.status, cell: ({ row }) => <StatusChip status={row.original.branch.status} />, meta: { className: 'w-32' } },
    {
      id: 'when', header: 'When', accessorFn: (r) => r.branch.created_at, meta: { className: 'w-28 text-right' },
      cell: ({ row }) => <span className="text-xs text-muted" title={formatTime(row.original.branch.created_at)}>{ago(row.original.branch.created_at)}</span>,
    },
    { id: 'menu', header: '', enableSorting: false, cell: ({ row }) => <RowMenu row={row.original} onDelete={onDelete} />, meta: { className: 'w-10 px-1' } },
  ];
}

/** Two-step delete: the run and every branch, freeing its saves. */
function DeleteRunDialog({ run, onClose, onDone }: { run: RunSummary | null; onClose: () => void; onDone: () => void }) {
  const { busy, error, run: act, reset } = useAction(async (r: RunSummary) => {
    const { dropped } = await deleteRun(r.id);
    window.dispatchEvent(new Event(DISK_CHANGED));
    toast(`Deleted ${runTitle(r)}`, { description: `${dropped} saved world${dropped === 1 ? '' : 's'} dropped.` });
    onDone(); onClose();
  });
  const remove = async () => { if (run) await act(run); };
  return (
    <Dialog open={!!run} onOpenChange={(o) => { if (!o) { reset(); onClose(); } }}>
      <DialogContent title="Delete this run?" description={run ? `${runTitle(run)} and its ${run.branches.length} branches. Can't be undone.` : ''}>
        {error && <ErrorState error={error} hint="Nothing was deleted." className="m-4" />}
        <DialogActions onCancel={onClose} onGo={remove} disabled={busy} danger label={busy ? 'Deleting…' : 'Delete run'} />
      </DialogContent>
    </Dialog>
  );
}

export default function Runs() {
  const nav = useNavigate();
  const { runs, error, refresh } = useRuns();
  const taskNames = useTaskNames();
  const [deleting, setDeleting] = useState<RunSummary | null>(null);
  const [sp, setSp] = useSearchParams();
  const filter = useMemo(() => parseRunFilter(sp), [sp]);
  const filtered = isFiltered(filter);
  const rows = useMemo(() => (runs ? branchRows(filterRuns(runs, filter)) : null), [runs, filter]);
  const clear = () => setSp({}, { replace: true });
  const nameOf = useCallback((r: BranchRow) => runTitle(r.run, taskNames), [taskNames]);
  const cols = useMemo(() => columns(nameOf, setDeleting), [nameOf]);
  const newRun = <Button variant="primary" asChild><Link to="/run"><Play />Run once</Link></Button>;
  return (
    <>
      <PageHeader title="Runs" width="default" actions={runs?.length ? newRun : null} />
      <PageBody width="default">
        {error && runs && <ErrorState error={error} onRetry={refresh} />}
        {filtered && (
          <Notice>
            <Hint hint="And their reruns.">{describeRunFilter(filter)}</Hint>{' '}
            <button type="button" onClick={clear} className="text-accent underline-offset-2 hover:underline">Show every run</button>
          </Notice>
        )}
        <Panel>
          <DataTable label="Runs" columns={cols} data={rows} getRowId={(r) => r.branch.id} loadingLabel="Loading runs…"
            error={error} onRetry={refresh} onOpen={(r) => nav(openOf(r))}
            empty={filtered && runs?.length
              ? <EmptyState title="No runs match" hint={describeRunFilter(filter)} action={<Button onClick={clear}>Show every run</Button>} />
              : <EmptyState title="No runs yet" action={newRun} />} />
        </Panel>
      </PageBody>
      <DeleteRunDialog run={deleting} onClose={() => setDeleting(null)} onDone={refresh} />
    </>
  );
}
