import type { CompareResponse } from '@/model/types';
import { get, qs } from './client';

export const compare = (a: string, b: string) => get<CompareResponse>('/api/compare' + qs({ a, b }));
