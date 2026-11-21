import { benchmark } from '@/api/benchmark';
import { useQuery } from './useQuery';

/** The benchmark's description (its name, tasks, disruptions, whether it draws a view): read again by each page that shows it. */
export const useBenchmark = () => useQuery('benchmark', benchmark, { shared: true, fresh: true });
