import { views as fetchViews } from '@/api/branches';
import { runs as fetchRuns } from '@/api/runs';
import { useBenchmark } from '@/hooks/useBenchmark';
import { useQuery } from '@/hooks/useQuery';
import type { TwinView } from '@/model/twin';

/** The digital twin as the newest run first saw it; null when nothing ran yet. */
async function newestTwin(): Promise<TwinView | null> {
  const newest = (await fetchRuns()).sort((x, y) => y.created_at.localeCompare(x.created_at))[0];
  const first = newest?.branches.find((br) => br.parent_id == null);
  if (!first) return null;
  return (await fetchViews(first.id))[0]?.view ?? null;
}

/**
 * The world before a run: the benchmark's name, and its digital twin as the newest run first saw it
 * (null when it draws none or nothing ran yet). The preview is optional: when it can't be read, the
 * world card shows its parts instead.
 */
export function useWorldPreview() {
  const { data: bench } = useBenchmark();
  const { data: twin } = useQuery(bench?.view ? 'world-preview' : null, newestTwin);
  return { bench, twin };
}
