import { MoreHorizontal } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { toast } from 'sonner';
import { exportRun, type Exported } from '@/api/observability';
import { openPalette } from '@/app/palette';
import { StatusChip } from '@/components/marks';
import { Button } from '@/components/ui/button';
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuTrigger } from '@/components/ui/dropdown-menu';
import { branchTree } from '@/features/runs/tree';
import { rewindSeq } from '@/features/world/scrub';
import { path } from '@/lib/url';
import type { Branch, BranchDetail } from '@/model/types';
import { withStep } from './useStepParam';

export type RunPage = 'details' | 'world' | 'process' | 'live';

const VIEWS: { page: Exclude<RunPage, 'live'>; label: string; path: string }[] = [
  { page: 'details', label: 'Timeline and details', path: 'runs' },
  { page: 'world', label: 'World view', path: 'world' },
  { page: 'process', label: 'Process view', path: 'control' },
];

/**
 * A run's "⋯": its other views, comparing it, its reruns, and rewinding it. Rewind lives here
 * only: it is for digging into one run, not part of testing agents.
 */
/** What an export did, in one line: per target sent, queued or failed; none set up says where to add one. */
export function exportedLine(r: Exported): string {
  if (r.exports.length === 0) return 'No observability target yet: add one in Settings → Observability.';
  return r.exports.map((e) => `${e.target}: ${e.state === 'done' ? `sent${e.spans != null ? ` ${e.spans} spans` : ''}` : e.state === 'queued' ? 'queued until the run ends' : `failed${e.error ? ` (${e.error})` : ''}`}`).join(' · ');
}

async function sendOut(branchId: string) {
  try {
    const r = await exportRun(branchId);
    const failed = r.exports.some((e) => e.state === 'failed');
    (failed ? toast.error : toast)(exportedLine(r));
  } catch (e) {
    toast.error(`Export failed: ${e instanceof Error ? e.message : String(e)}`);
  }
}

export default function RunMenu({ runId, d, page, step, siblings = [] }: {
  runId: string; d: BranchDetail; page: RunPage; step: number | null; siblings?: Branch[];
}) {
  const nav = useNavigate();
  const branchId = d.id;
  const running = d.status === 'running';
  // The first step at or after the one picked that a rerun can start from (the first agent step when none is).
  const rewindAt = rewindSeq(d.events, (step ?? 0) - 1);
  const at = path`${runId}/${branchId}`;
  const reruns = siblings.length > 1 ? branchTree(siblings) : [];
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild><Button variant="ghost" size="icon" aria-label="More about this run"><MoreHorizontal /></Button></DropdownMenuTrigger>
      <DropdownMenuContent className="w-64">
        {VIEWS.filter((v) => v.page !== page).map((v) => (
          <DropdownMenuItem key={v.page} onSelect={() => nav(withStep(`/${v.path}/${at}`, step))}>{v.label}</DropdownMenuItem>
        ))}
        <DropdownMenuItem onSelect={() => openPalette({ kind: 'compare-runs', first: branchId })}>Compare with another run…</DropdownMenuItem>
        <DropdownMenuItem onSelect={() => void sendOut(branchId)}>Export this run (OpenTelemetry)</DropdownMenuItem>
        {reruns.length > 0 && <DropdownMenuLabel>Reruns</DropdownMenuLabel>}
        {reruns.map(({ branch: b, depth }) => (
          <DropdownMenuItem key={b.id} onSelect={() => nav(path`/runs/${runId}/${b.id}`)} className={b.id === branchId ? 'bg-accent-soft' : ''}>
            <span style={{ paddingLeft: depth * 12 }} className="min-w-0 flex-1 truncate text-xs">{depth ? `↳ ${b.fork_note ?? 'Rerun'}` : 'Original'}</span>
            <StatusChip status={b.status} />
          </DropdownMenuItem>
        ))}
        <DropdownMenuLabel>Advanced</DropdownMenuLabel>
        <DropdownMenuItem disabled={running || rewindAt == null} onSelect={() => rewindAt != null && nav(`/runs/${at}?step=${rewindAt}&fork=1`)}>
          {running ? 'Rewind (when the run ends)' : rewindAt == null ? 'Rewind (no step to rerun from)' : `Rewind to step ${rewindAt} and rerun…`}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
