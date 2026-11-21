// `GET /api/me`: who is using this shaked, and what the deployment's layer adds (an account menu).
import { get } from './client';

export interface AccountItem {
  label: string;
  current?: boolean;
  /** Where choosing it POSTs `body` (then the page reloads). */
  post?: string;
  body?: unknown;
}

export interface Me {
  mode: string;
  user: { name: string };
  features: string[];
  account?: { label: string; items: AccountItem[] };
}

export const fetchMe = () => get<Me>('/api/me');
