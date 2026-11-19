import { benchmark } from '@/api/benchmark';
import { useQuery } from './useQuery';

/** The benchmark's description (its name, tasks, disruptions, whether it draws a view): read once per page. */
export const useBenchmark = () => useQuery('benchmark', benchmark, { shared: true });
