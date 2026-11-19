import { useSyncExternalStore } from 'react';
import { reviewSettings } from '@/api/reviews';
import { useQuery } from '@/hooks/useQuery';
import type { ReviewSettings } from '@/model/review';

/** This browser's choice (⋯ → AI review): on unless turned off. */
export const REVIEW_KEY = 'arena.review';

/** The stored choice read as on or off: anything but "off" is on. */
export const parseOn = (v: string | null) => v !== 'off';

function readOn(): boolean {
  try {
    return parseOn(localStorage.getItem(REVIEW_KEY));
  } catch {
    return true;
  }
}

let on = readOn();
const listeners = new Set<() => void>();

export function setReviewOn(next: boolean) {
  on = next;
  try {
    localStorage.setItem(REVIEW_KEY, next ? 'on' : 'off');
  } catch {
    /* storage unavailable: the choice lasts for this page */
  }
  listeners.forEach((f) => f());
}

const subscribe = (f: () => void) => { listeners.add(f); return () => { listeners.delete(f); }; };

/** This browser's choice, kept in step across the page. */
export const useReviewOn = () => useSyncExternalStore(subscribe, () => on, () => on);

/** arenad's settings, asked once per page: null while asking; a failure counts as off, and is asked again next time. */
export function useServerReview(): ReviewSettings | null {
  const { data, error } = useQuery('review-settings', reviewSettings, { shared: true });
  return data ?? (error != null ? { enabled: false, model: '', why: error } : null);
}

/** Whether review cards show: on in this browser and in arenad. */
export function useReviewEnabled(): boolean {
  const mine = useReviewOn();
  const s = useServerReview();
  return mine && !!s?.enabled;
}
