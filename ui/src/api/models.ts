// `/api/models/keys`: your model keys and OpenAI-compatible servers. A secret goes in and never
// comes back: lists show its last four characters.
import { del, get, path, post } from './client';

export type KeyKind = 'anthropic' | 'openai' | 'openrouter' | 'bedrock' | 'custom';

export interface KeyRow {
  id: string;
  name: string;
  kind: KeyKind;
  base_url: string | null;
  model: string | null;
  key_last4: string;
  checked_ok: boolean | null;
  checked_at: string | null;
  check_detail: string;
  created_at: string;
}

export interface NewKey { name: string; kind: KeyKind; secret: string; base_url?: string; model?: string }

export const listKeys = () => get<KeyRow[]>('/api/models/keys');
export const addKey = (k: NewKey) => post<KeyRow>('/api/models/keys', k);
export const testKey = (id: string) => post<KeyRow>(path`/api/models/keys/${id}/test`);
export const replaceSecret = (id: string, secret: string) => post<{ replaced: string }>(path`/api/models/keys/${id}/secret`, { secret });
export const deleteKey = (id: string) => del<{ deleted: string }>(path`/api/models/keys/${id}`);
