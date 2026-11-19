import type { CompareReview, ReviewSettings, RunReview, StoredReview, Target } from '@/model/review';
import { ApiError, get, path, post, qs } from './client';

/** The route of a target's review. */
export function reviewPath(t: Target): string {
  switch (t.kind) {
    case 'run': return path`/api/branches/${t.branch}/review`;
    case 'compare': return '/api/compare/review' + qs({ a: t.a, b: t.b });
    case 'setups': return '/api/suites/compare/review' + qs({ a: t.a, b: t.b });
  }
}

const withAgain = (p: string) => `${p}${p.includes('?') ? '&' : '?'}again=1`;

export const reviewSettings = () => get<ReviewSettings>('/api/review/settings');

/** The stored review, or null when there is none yet. */
export async function getReview<R extends RunReview | CompareReview>(t: Target): Promise<StoredReview<R> | null> {
  try {
    return await get<StoredReview<R>>(reviewPath(t));
  } catch (e) {
    if (e instanceof ApiError && e.status === 404) return null;
    throw e;
  }
}

/** Starts a review (or gets the stored one); `again` asks for a new one. */
export const startReview = <R extends RunReview | CompareReview>(t: Target, again = false) =>
  post<StoredReview<R>>(again ? withAgain(reviewPath(t)) : reviewPath(t));
