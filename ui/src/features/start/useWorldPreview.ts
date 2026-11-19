import { useEffect, useState } from 'react';
import { benchmark } from '@/api/benchmark';
import { views as fetchViews } from '@/api/branches';
import { runs as fetchRuns } from '@/api/runs';
import type { TwinView } from '@/model/twin';
import type { BenchmarkDescription } from '@/model/types';

/**
 * The world before a run: the benchmark's name, and its digital twin as the newest run first saw it
 * (null when it draws none or nothing ran yet).
 */
export function useWorldPreview() {
  const [bench, setBench] = useState<BenchmarkDescription | null>(null);
  const [twin, setTwin] = useState<TwinView | null>(null);
  useEffect(() => {
    let alive = true;
    (async () => {
      const b = await benchmark();
      if (!alive) return;
      setBench(b);
      if (!b.view) return;
      const newest = (await fetchRuns()).sort((x, y) => y.created_at.localeCompare(x.created_at))[0];
      const first = newest?.branches.find((br) => br.parent_id == null);
      if (!first) return;
      const v = await fetchViews(first.id);
      if (alive) setTwin(v[0]?.view ?? null);
    })().catch(() => { /* the preview is optional: the world card shows its parts instead */ });
    return () => { alive = false; };
  }, []);
  return { bench, twin };
}
