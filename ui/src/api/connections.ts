// Code connections (`/api/connections`): saved commands that start a team's own driver program.
import type { Connection, ConnectionTest, NewConnection } from '@/model/connection';
import { del, get, path, post } from './client';

export const listConnections = () => get<Connection[]>('/api/connections');
export const createConnection = (body: NewConnection) => post<Connection>('/api/connections', body);
export const deleteConnection = (id: string) => del<{ deleted: string }>(path`/api/connections/${id}`);
export const testConnection = (id: string) => post<ConnectionTest>(path`/api/connections/${id}/test`, {});
