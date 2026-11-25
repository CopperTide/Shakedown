// `/api/observability`: where runs are exported as OpenTelemetry traces (Langfuse, Arize Phoenix,
// any OTLP collector), what was exported, and exporting a run now. Header values go in and never
// come back: lists show their names.
import { del, get, path, post } from './client';

export type TargetKind = 'langfuse' | 'phoenix' | 'otlp';

export interface TargetRow {
  id: string;
  name: string;
  kind: TargetKind;
  endpoint: string;
  project: string | null;
  header_names: string[];
  include_content: boolean;
  auto_export: boolean;
  export_metrics: boolean;
  checked_ok: boolean | null;
  checked_at: string | null;
  check_detail: string;
  created_at: string;
}

/** What `POST /api/observability/targets` takes; each preset reads its own fields. */
export interface NewTarget {
  name: string;
  kind: TargetKind;
  region?: 'us' | 'eu' | 'self';
  base_url?: string;
  public_key?: string;
  secret_key?: string;
  api_key?: string;
  project?: string;
  endpoint?: string;
  headers?: Record<string, string>;
  include_content?: boolean;
  auto_export?: boolean;
  export_metrics?: boolean;
}

export type TargetFlags = Pick<TargetRow, 'include_content' | 'auto_export' | 'export_metrics'>;

export interface ExportRow {
  target_id: string;
  target: string;
  branch_id: string;
  state: 'pending' | 'done' | 'failed';
  attempts: number;
  spans: number | null;
  last_error: string;
  exported_at: string | null;
  next_at: string;
}

export interface Exported {
  branch_id: string;
  exports: { target_id: string; target: string; state: 'done' | 'failed' | 'queued'; spans?: number; error?: string }[];
}

export const listTargets = () => get<TargetRow[]>('/api/observability/targets');
export const addTarget = (t: NewTarget) => post<TargetRow>('/api/observability/targets', t);
export const setTargetFlags = (id: string, f: Partial<TargetFlags>) => post<TargetRow>(path`/api/observability/targets/${id}`, f);
export const testTarget = (id: string) => post<TargetRow>(path`/api/observability/targets/${id}/test`);
export const deleteTarget = (id: string) => del<{ deleted: string }>(path`/api/observability/targets/${id}`);
export const listExports = () => get<ExportRow[]>('/api/observability/exports');
/** Sends a branch to every target (or one) now; queued when it has not ended. */
export const exportRun = (branchId: string, targetId?: string) =>
  post<Exported>(path`/api/branches/${branchId}/export`, targetId ? { target_id: targetId } : {});
