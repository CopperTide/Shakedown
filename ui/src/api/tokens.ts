// `/api/tokens`: API tokens for the SDK and scripts. A new token is in the answer once.
import { del, get, path, post } from './client';

export type TokenScope = 'read' | 'run' | 'admin';

export interface TokenRow {
  id: string;
  name: string;
  prefix: string;
  scopes: TokenScope[];
  created_at: string;
  last_used_at: string | null;
  expires_at: string | null;
}

export interface Minted { id: string; token: string; prefix: string }

export const listTokens = () => get<TokenRow[]>('/api/tokens');
export const createToken = (name: string, scopes: TokenScope[], expires_days?: number) =>
  post<Minted>('/api/tokens', { name, scopes, ...(expires_days ? { expires_days } : {}) });
export const revokeToken = (id: string) => del<{ revoked: string }>(path`/api/tokens/${id}`);
